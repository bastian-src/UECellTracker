use anyhow::Result;
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
