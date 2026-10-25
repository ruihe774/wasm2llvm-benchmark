//! w2l-ref: reference implementation of the w2l harness protocol backed by wasmtime.
//!
//! It reads the same line-based commands from stdin as the native harness
//! (`harness/w2l_harness.c`), but `load` takes a `.wasm` file instead of a
//! translated `.so`. See `harness/PROTOCOL.md` for the protocol.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use wasmtime::*;

struct Host {
    out: Vec<u8>,
}

struct Inst {
    module: Module,
    store: Store<Host>,
    instance: Option<Instance>,
}

fn engine() -> Engine {
    let mut c = Config::new();
    // Exactly the feature set of the benchmark.
    {
        use wasmtime::WasmFeatures as F;
        c.wasm_features(F::all(), false);
        c.wasm_features(
            F::MUTABLE_GLOBAL
                | F::SATURATING_FLOAT_TO_INT
                | F::SIGN_EXTENSION
                | F::MULTI_VALUE
                | F::BULK_MEMORY
                | F::SIMD
                | F::FLOATS,
            true,
        );
    }
    c.cranelift_nan_canonicalization(false);
    Engine::new(&c).expect("engine")
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        bail!("odd hex length");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format_err!("{}", e)))
        .collect()
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn decode_name(tok: &str) -> Result<String> {
    // "-" encodes the empty name (the native harness accepts the same).
    let bytes = if tok == "-" { vec![] } else { hex_decode(tok)? };
    String::from_utf8(bytes).map_err(|e| format_err!("{}", e))
}

fn slot_to_val(slot: &[u8; 16], ty: &ValType) -> Result<Val> {
    let u32_ = u32::from_le_bytes(slot[0..4].try_into().unwrap());
    let u64_ = u64::from_le_bytes(slot[0..8].try_into().unwrap());
    Ok(match ty {
        ValType::I32 => Val::I32(u32_ as i32),
        ValType::I64 => Val::I64(u64_ as i64),
        ValType::F32 => Val::F32(u32_),
        ValType::F64 => Val::F64(u64_),
        ValType::V128 => Val::V128(u128::from_le_bytes(*slot).into()),
        _ => bail!("unsupported value type"),
    })
}

fn val_to_slot(v: &Val) -> Result<[u8; 16]> {
    let mut s = [0u8; 16];
    match v {
        Val::I32(x) => s[0..4].copy_from_slice(&x.to_le_bytes()),
        Val::I64(x) => s[0..8].copy_from_slice(&x.to_le_bytes()),
        Val::F32(x) => s[0..4].copy_from_slice(&x.to_le_bytes()),
        Val::F64(x) => s[0..8].copy_from_slice(&x.to_le_bytes()),
        Val::V128(x) => s = x.as_u128().to_le_bytes(),
        _ => bail!("unsupported value type"),
    }
    Ok(s)
}

fn trap_code(e: &wasmtime::Error) -> Option<u32> {
    let t = e.downcast_ref::<Trap>()?;
    Some(match t {
        Trap::UnreachableCodeReached => 0,
        Trap::IntegerDivisionByZero => 1,
        Trap::IntegerOverflow => 2,
        Trap::BadConversionToInteger => 3,
        Trap::MemoryOutOfBounds | Trap::HeapMisaligned => 4,
        Trap::TableOutOfBounds => 5,
        Trap::IndirectCallToNull => 6,
        Trap::BadSignature => 7,
        Trap::StackOverflow => 8,
        _ => 9,
    })
}

fn fmt_val(v: &Val) -> String {
    match v {
        Val::I32(x) => format!("i32:{}\n", x),
        Val::I64(x) => format!("i64:{}\n", x),
        Val::F32(x) => format!("f32:0x{:08x}\n", x),
        Val::F64(x) => format!("f64:0x{:016x}\n", x),
        Val::V128(x) => format!("v128:{}\n", hex_encode(&x.as_u128().to_le_bytes())),
        _ => String::new(),
    }
}

/// Host functions available for import. Must match the native harness.
fn host_funcs() -> Vec<(&'static str, &'static str, Vec<ValType>)> {
    use ValType::*;
    let mut v = vec![
        ("spectest", "print", vec![]),
        ("spectest", "print_i32", vec![I32]),
        ("spectest", "print_i64", vec![I64]),
        ("spectest", "print_f32", vec![F32]),
        ("spectest", "print_f64", vec![F64]),
        ("spectest", "print_i32_f32", vec![I32, F32]),
        ("spectest", "print_f64_f64", vec![F64, F64]),
    ];
    for (n, t) in [
        ("print_i32", I32),
        ("print_i64", I64),
        ("print_f32", F32),
        ("print_f64", F64),
        ("print_v128", V128),
        ("putchar", I32),
    ] {
        v.push(("env", n, vec![t]));
    }
    v
}

fn make_linker(engine: &Engine) -> Result<Linker<Host>> {
    let mut l: Linker<Host> = Linker::new(engine);
    for (m, n, params) in host_funcs() {
        let ty = FuncType::new(engine, params, []);
        let is_putchar = n == "putchar";
        let is_print = n == "print";
        l.func_new(m, n, ty, move |mut c, args, _res| {
            let out = &mut c.data_mut().out;
            if is_putchar {
                if let Val::I32(x) = args[0] {
                    out.push(x as u8);
                }
            } else if is_print {
                out.extend_from_slice(b"print\n");
            } else {
                for a in args {
                    out.extend_from_slice(fmt_val(a).as_bytes());
                }
            }
            Ok(())
        })?;
    }
    Ok(l)
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn handle(
    engine: &Engine,
    linker: &Linker<Host>,
    insts: &mut HashMap<String, Inst>,
    toks: &[&str],
) -> Result<String> {
    match toks {
        ["load", id, path] => {
            let module = Module::from_file(engine, path)?;
            let store = Store::new(engine, Host { out: vec![] });
            insts.insert(id.to_string(), Inst { module, store, instance: None });
            Ok("ok".into())
        }
        ["init", id] => {
            let i = insts.get_mut(*id).ok_or_else(|| format_err!("no such module"))?;
            match linker.instantiate(&mut i.store, &i.module) {
                Ok(inst) => {
                    i.instance = Some(inst);
                    Ok("ok".into())
                }
                Err(e) => match trap_code(&e) {
                    Some(c) => Ok(format!("trap {}", c)),
                    None => Err(e),
                },
            }
        }
        ["invoke", id, name, _nres, args @ ..] => {
            let i = insts.get_mut(*id).ok_or_else(|| format_err!("no such module"))?;
            let inst = i.instance.ok_or_else(|| format_err!("not initialized"))?;
            let f = inst
                .get_func(&mut i.store, &decode_name(name)?)
                .ok_or_else(|| format_err!("no such export"))?;
            let ty = f.ty(&i.store);
            let params: Vec<ValType> = ty.params().collect();
            if params.len() != args.len() {
                bail!("arity mismatch");
            }
            let mut vals = vec![];
            for (a, t) in args.iter().zip(params.iter()) {
                let b = hex_decode(a)?;
                let slot: [u8; 16] = b.try_into().map_err(|_| format_err!("bad slot"))?;
                vals.push(slot_to_val(&slot, t)?);
            }
            let mut res: Vec<Val> = ty.results().map(|_| Val::I32(0)).collect();
            match f.call(&mut i.store, &vals, &mut res) {
                Ok(()) => {
                    let mut s = String::from("ok");
                    for r in &res {
                        s.push(' ');
                        s.push_str(&hex_encode(&val_to_slot(r)?));
                    }
                    Ok(s)
                }
                Err(e) => match trap_code(&e) {
                    Some(c) => Ok(format!("trap {}", c)),
                    None => Err(e),
                },
            }
        }
        ["global", id, name] => {
            let i = insts.get_mut(*id).ok_or_else(|| format_err!("no such module"))?;
            let inst = i.instance.ok_or_else(|| format_err!("not initialized"))?;
            let g = inst
                .get_global(&mut i.store, &decode_name(name)?)
                .ok_or_else(|| format_err!("no such export"))?;
            let v = g.get(&mut i.store);
            Ok(format!("ok {}", hex_encode(&val_to_slot(&v)?)))
        }
        ["memhash", id, name] => {
            let i = insts.get_mut(*id).ok_or_else(|| format_err!("no such module"))?;
            let inst = i.instance.ok_or_else(|| format_err!("not initialized"))?;
            let m = inst
                .get_memory(&mut i.store, &decode_name(name)?)
                .ok_or_else(|| format_err!("no such export"))?;
            let d = m.data(&i.store);
            Ok(format!("ok {} {:016x}", d.len(), fnv1a64(d)))
        }
        ["output", id] => {
            let i = insts.get_mut(*id).ok_or_else(|| format_err!("no such module"))?;
            let o = std::mem::take(&mut i.store.data_mut().out);
            Ok(format!("ok {}", if o.is_empty() { "-".into() } else { hex_encode(&o) }))
        }
        ["ping"] => Ok("ok".into()),
        _ => bail!("bad command"),
    }
}

fn main() {
    let engine = engine();
    let linker = make_linker(&engine).expect("linker");
    let mut insts: HashMap<String, Inst> = HashMap::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin");
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() {
            continue;
        }
        if toks[0] == "quit" {
            break;
        }
        let reply = match handle(&engine, &linker, &mut insts, &toks) {
            Ok(s) => s,
            Err(e) => format!("error {}", format!("{:#}", e).replace('\n', " ")),
        };
        writeln!(stdout, "{}", reply).unwrap();
        stdout.flush().unwrap();
    }
}
