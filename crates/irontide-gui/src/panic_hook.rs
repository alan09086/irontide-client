/// Install a panic hook that logs and aborts.
///
/// M161's periodic auto-save writes .resume files every 300s, so most
/// state is already persisted. This hook ensures a clean abort rather
/// than unwinding through FFI (Slint) boundaries.
pub fn install() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort: log via eprintln (sync, no allocation needed).
        eprintln!("irontide-gui panicked: {info}");
        // Call the default hook for backtrace output.
        default_hook(info);
        // Abort rather than unwind — unwinding through Slint's event
        // loop or FFI boundaries is undefined behaviour.
        std::process::abort();
    }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn panic_hook_installs() {
        // Verify set_hook + take_hook round-trip doesn't panic.
        // We can't test the actual abort behaviour, but we can verify
        // the hook installs without error.
        let original = std::panic::take_hook();
        super::install();
        // Restore original hook so we don't interfere with test harness.
        let _installed = std::panic::take_hook();
        std::panic::set_hook(original);
    }
}
