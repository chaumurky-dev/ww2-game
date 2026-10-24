use leptos::prelude::*;
use std::time::Duration;

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

pub fn format_remaining(ms: f64) -> String {
    if ms <= 0.0 {
        return "expired".into();
    }
    let s = (ms / 1000.0) as u64;
    let (d, h, m, sec) = (s / 86400, s % 86400 / 3600, s % 3600 / 60, s % 60);
    if d > 0 {
        format!("{d}d {h}h {m:02}m")
    } else if h > 0 {
        format!("{h}h {m:02}m {sec:02}s")
    } else {
        format!("{m}m {sec:02}s")
    }
}

/// Ticking countdown to an epoch-millisecond deadline.
#[component]
pub fn Countdown(deadline_ms: f64) -> impl IntoView {
    let now = RwSignal::new(now_ms());
    if let Ok(handle) = set_interval_with_handle(move || now.set(now_ms()), Duration::from_secs(1)) {
        on_cleanup(move || handle.clear());
    }
    let left = move || deadline_ms - now.get();
    view! {
        <span class="countdown" class:urgent=move || left() < 3_600_000.0 title="Time left to act">
            {move || format_remaining(left())}
        </span>
    }
}
