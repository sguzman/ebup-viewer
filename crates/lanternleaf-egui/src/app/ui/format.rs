pub(crate) fn format_duration_secs(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "n/a".to_string();
    }
    let total = seconds.max(0.0).round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    if mins > 0 {
        format!("{mins}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

pub(crate) fn format_bytes(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "n/a".to_string();
    };
    let kb = 1024.0;
    let mb = kb * 1024.0;
    let gb = mb * 1024.0;
    let value = bytes as f64;
    if value >= gb {
        format!("{:.2} GB", value / gb)
    } else if value >= mb {
        format!("{:.2} MB", value / mb)
    } else if value >= kb {
        format!("{:.2} KB", value / kb)
    } else {
        format!("{} B", bytes)
    }
}

pub(crate) fn format_relative_unix_secs(unix_secs: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(unix_secs);
    if unix_secs >= now_secs {
        return "just now".to_string();
    }
    let delta = now_secs - unix_secs;
    let mins = delta / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if mins > 0 {
        format!("{}m ago", mins)
    } else {
        format!("{}s ago", delta)
    }
}
