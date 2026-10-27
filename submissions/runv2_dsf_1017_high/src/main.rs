mod codegen;
mod parse;
mod validate;

use std::process::exit;

fn usage() -> ! {
    eprintln!("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-o" {
            i += 1;
            if i >= args.len() {
                usage();
            }
            output = Some(args[i].clone());
        } else if let Some(rest) = a.strip_prefix("-o") {
            output = Some(rest.to_string());
        } else if a.starts_with('-') && a.len() > 1 {
            eprintln!("wasm2llvm: unknown option {}", a);
            exit(2);
        } else if input.is_none() {
            input = Some(a.clone());
        } else {
            eprintln!("wasm2llvm: unexpected argument {}", a);
            exit(2);
        }
        i += 1;
    }
    let input = match input {
        Some(i) => i,
        None => usage(),
    };
    let output = match output {
        Some(o) => o,
        None => usage(),
    };

    let data = match std::fs::read(&input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("wasm2llvm: cannot read {}: {}", input, e);
            exit(1);
        }
    };

    let module = match parse::parse_module(&data) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("wasm2llvm: invalid module: {}", e);
            exit(1);
        }
    };
    if let Err(e) = validate::validate(&module) {
        eprintln!("wasm2llvm: invalid module: {}", e);
        exit(1);
    }
    let ir = match codegen::generate(&module) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("wasm2llvm: cannot translate: {}", e);
            exit(1);
        }
    };
    if let Err(e) = std::fs::write(&output, ir) {
        eprintln!("wasm2llvm: cannot write {}: {}", output, e);
        exit(1);
    }
}
