use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn prepare_sigint_notifier() -> Result<Arc<AtomicBool>> {
    let notifier = Arc::new(AtomicBool::new(false));
    let r = notifier.clone();
    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    Ok(notifier)
}

pub fn is_notifier(notifier: &Arc<AtomicBool>) -> bool {
    notifier.load(Ordering::SeqCst)
}

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
