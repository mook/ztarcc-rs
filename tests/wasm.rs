use anyhow::Result;
use wasm_bindgen_test::*;
use ztarcc_rs::{convert, Script};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_convert() -> Result<()> {
    let input = "我能吞下玻璃而不伤身体。";
    let expected = "我能吞下玻璃而不傷身體。";
    let result = convert(Script::CN, Script::TW, input)?;
    assert_eq!(expected, result.join(""));
    Ok(())
}
