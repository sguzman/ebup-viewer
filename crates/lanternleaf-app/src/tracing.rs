use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

fn timestamp_slug() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(now) => format!("{}-{:03}", now.as_secs(), now.subsec_millis()),
        Err(_) => "0-000".to_string(),
    }
}

/// Initializes the global tracing subscriber with a rolling log file and the provided fallback
/// level. Returns the guard that must be held for the lifetime of the application to keep the
/// background writer alive.
pub fn init_tracing(default_level: impl AsRef<str>) -> WorkerGuard {
    let default_directive = default_level.as_ref().to_string();
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&default_directive))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let logs_dir = "logs";
    if let Err(err) = fs::create_dir_all(logs_dir) {
        eprintln!("failed to create tracing logs dir {logs_dir}: {err}");
    }
    let log_name = format!("lanternleaf-egui-{}.log", timestamp_slug());
    let file_appender = tracing_appender::rolling::never(logs_dir, log_name);
    let (file_writer, guard) = non_blocking(file_appender);

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_file(true)
        .with_line_number(true);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(file_writer)
        .with_target(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

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
