use std::{env, fs, process};

mod wasm;
mod validate;
mod simd;
mod codegen;
mod function;
mod numeric;
mod memory_codegen;
mod simd_codegen;
mod simd_arith;
mod translator;

fn main() {
    if let Err(error) = run() {
        eprintln!("wasm2llvm: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let input = args.next().ok_or_else(|| "usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".to_string())?;
    let flag = args.next().ok_or_else(|| "usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".to_string())?;
    let output = args.next().ok_or_else(|| "usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".to_string())?;
    if flag != "-o" || args.next().is_some() {
        return Err("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".to_string());
    }
    let bytes = fs::read(&input).map_err(|e| format!("cannot read {}: {e}", input.to_string_lossy()))?;
    let ir = translator::translate(&bytes)?;
    fs::write(&output, ir).map_err(|e| format!("cannot write {}: {e}", output.to_string_lossy()))?;
    Ok(())
}
