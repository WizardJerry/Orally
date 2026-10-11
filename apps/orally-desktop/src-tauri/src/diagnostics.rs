//! Desktop lifetime and settings integration for content-free diagnostics.
use orally_config::LoggingConfig;

pub(super) fn apply(config: &LoggingConfig) {
    let result = std::env::current_exe().and_then(|exe| {
        let directory = exe
            .parent()
            .ok_or_else(|| std::io::Error::other("missing application directory"))?
            .join("logs");
        orally_diagnostics::configure(directory, config.auto_cleanup, config.retention_days.get())
    });
    if result.is_err() {
        eprintln!("Orally diagnostic log setup or cleanup failed");
    }
}

pub(super) fn start() {
    let config = super::tray::with_config_lock(|| {
        orally_config::load_or_default().map_err(|error| error.to_string())
    });
    // A temporarily invalid settings file must not silently enable deletion.
    let logging = config.map(|c| c.logging).unwrap_or_else(|_| LoggingConfig {
        auto_cleanup: false,
        ..LoggingConfig::default()
    });
    apply(&logging);
    orally_diagnostics::session_started();
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if orally_diagnostics::maintain().is_err() {
            eprintln!("Orally diagnostic log cleanup failed");
        }
    });
}
