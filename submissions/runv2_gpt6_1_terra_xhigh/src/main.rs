mod wasm;
mod numeric;
mod simd;
mod codegen;
mod module;

use std::ffi::OsString;
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let mut args=std::env::args_os().skip(1);
    let input=args.next().ok_or("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll")?;
    if input==OsString::from("-o") {return Err("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".into())}
    if args.next()!=Some(OsString::from("-o")) {return Err("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".into())}
    let output=args.next().ok_or("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll")?;
    if args.next().is_some() {return Err("usage: wasm2llvm INPUT.wasm -o OUTPUT.ll".into())}
    let bytes=std::fs::read(&input).map_err(|e|format!("cannot read {}: {e}",PathBuf::from(&input).display()))?;
    let m=wasm::parse(&bytes).map_err(|e|e.to_string())?;
    let ir=module::translate(&m)?;
    std::fs::write(&output,ir).map_err(|e|format!("cannot write {}: {e}",PathBuf::from(&output).display()))?;
    Ok(())
}
fn main() {if let Err(e)=run(){eprintln!("wasm2llvm: {e}");std::process::exit(1)}}
