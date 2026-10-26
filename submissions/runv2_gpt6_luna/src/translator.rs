use crate::{codegen, validate, wasm};

pub fn translate(bytes: &[u8]) -> Result<String, String> {
    let mut module = wasm::parse(bytes)?;
    validate::validate_module(&mut module)?;
    codegen::emit(&module)
}
