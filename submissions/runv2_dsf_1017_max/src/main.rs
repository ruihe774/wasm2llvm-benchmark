mod codegen;
mod instr;
mod module;
mod num;
mod reader;
mod validate;

use std::process::exit;

fn fail(msg: &str) -> ! {
    eprintln!("wasm2llvm: {}", msg);
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "-o" {
            if i + 1 >= args.len() {
                fail("option -o requires an argument");
            }
            output = Some(args[i + 1].clone());
            i += 2;
        } else if let Some(rest) = a.strip_prefix("-o") {
            if rest.is_empty() {
                fail("option -o requires an argument");
            }
            output = Some(rest.to_string());
            i += 1;
        } else if a == "-h" || a == "--help" {
            println!("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll");
            return;
        } else if input.is_none() {
            input = Some(a.clone());
            i += 1;
        } else {
            fail(&format!("unexpected argument: {}", a));
        }
    }
    let input = match input {
        Some(x) => x,
        None => fail("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll"),
    };
    let output = match output {
        Some(x) => x,
        None => fail("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll"),
    };

    let data = match std::fs::read(&input) {
        Ok(d) => d,
        Err(e) => fail(&format!("cannot read {}: {}", input, e)),
    };
    let module = match module::parse_module(&data) {
        Ok(m) => m,
        Err(e) => fail(&format!("invalid module: {}", e)),
    };
    if let Err(e) = validate::validate_module(&module) {
        fail(&format!("invalid module: {}", e));
    }
    let ir = match codegen::generate(&module) {
        Ok(x) => x,
        Err(e) => fail(&format!("cannot translate module: {}", e)),
    };
    if let Err(e) = std::fs::write(&output, ir) {
        fail(&format!("cannot write {}: {}", output, e));
    }
}
