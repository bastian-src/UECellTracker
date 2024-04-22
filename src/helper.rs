use anyhow::{anyhow, Result};

pub fn helper_json_pointer(
    nested_value: &serde_json::Value,
    pointer: &str,
) -> Result<serde_json::Value> {
    match nested_value.pointer(pointer) {
        Some(unnested_value) => Ok(unnested_value.clone()),
        _ => Err(anyhow!(
            "No value found by '{:?}' in {:#?}",
            pointer,
            nested_value
        )),
    }
}
