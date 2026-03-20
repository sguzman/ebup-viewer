use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::{EnvFilter, fmt};

/// Initializes the global tracing subscriber with a rolling log file and the provided fallback
/// level. Returns the guard that must be held for the lifetime of the application to keep the
/// background writer alive.
pub fn init_tracing(default_level: impl AsRef<str>) -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily("logs", "lanternleaf.log");
    let (non_blocking, guard) = non_blocking(file_appender);

    let default_directive = default_level.as_ref().to_string();
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&default_directive))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = fmt::fmt()
        .with_env_filter(env_filter)
        .with_writer(non_blocking)
        .with_thread_names(true)
        .with_target(true)
        .with_level(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set tracing subscriber for lanternleaf-app");

    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_tracing_returns_guard() {
        let guard = init_tracing("debug");
        drop(guard);
    }
}
