mod ir;
mod model;
mod parse;
mod simd;
mod validate;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || args[1] != "-o" {
        return Err("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".into());
    }
    let bytes = std::fs::read(&args[0]).map_err(|e| format!("input: {e}"))?;
    let module = parse::parse(&bytes)?;
    let output = ir::translate(&module)?;
    std::fs::write(&args[2], output).map_err(|e| format!("output: {e}"))
}
fn main() {
    if let Err(e) = run() {
        eprintln!("wasm2llvm: {e}");
        std::process::exit(1);
    }
}
