//! Time-zone helpers: browser zone list/default (`Intl`, via `js_sys::Reflect`), a `<select>`
//! component and "Tue 14:05" deadline formatting in the user's zone.
use crate::state::AuthState;
use js_sys::{Array, Date, Function, Object, Reflect};
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

/// Used when `Intl.supportedValuesOf` is unavailable.
pub const FALLBACK_ZONES: [&str; 24] = [
    "UTC",
    "America/Los_Angeles",
    "America/Denver",
    "America/Chicago",
    "America/New_York",
    "America/Sao_Paulo",
    "America/Argentina/Buenos_Aires",
    "Atlantic/Reykjavik",
    "Europe/London",
    "Europe/Berlin",
    "Europe/Paris",
    "Europe/Moscow",
    "Africa/Cairo",
    "Africa/Johannesburg",
    "Asia/Dubai",
    "Asia/Kolkata",
    "Asia/Bangkok",
    "Asia/Shanghai",
    "Asia/Tokyo",
    "Asia/Seoul",
    "Australia/Sydney",
    "Pacific/Auckland",
    "Pacific/Honolulu",
    "America/Anchorage",
];

fn get(obj: &JsValue, key: &str) -> Option<JsValue> {
    Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

fn intl() -> Option<JsValue> {
    get(&js_sys::global(), "Intl")
}

/// `Intl.DateTimeFormat().resolvedOptions().timeZone`.
pub fn browser_timezone() -> Option<String> {
    let dtf: Function = get(&intl()?, "DateTimeFormat")?.dyn_into().ok()?;
    let inst = Reflect::construct(&dtf, &Array::new()).ok()?;
    let resolved: Function = get(&inst, "resolvedOptions")?.dyn_into().ok()?;
    let opts = resolved.call0(&inst).ok()?;
    get(&opts, "timeZone")?.as_string()
}

/// `Intl.supportedValuesOf('timeZone')`, or a short static list.
pub fn supported_timezones() -> Vec<String> {
    let from_intl = || -> Option<Vec<String>> {
        let intl = intl()?;
        let f: Function = get(&intl, "supportedValuesOf")?.dyn_into().ok()?;
        let arr: Array = f
            .call1(&intl, &JsValue::from_str("timeZone"))
            .ok()?
            .dyn_into()
            .ok()?;
        Some(arr.iter().filter_map(|v| v.as_string()).collect())
    };
    let mut zones = from_intl()
        .filter(|z| !z.is_empty())
        .unwrap_or_else(|| FALLBACK_ZONES.iter().map(|s| s.to_string()).collect());
    if !zones.iter().any(|z| z == "UTC") {
        zones.insert(0, "UTC".into());
    }
    zones
}

/// Zone list for a select, always containing `current` and the browser zone.
pub fn zone_options(current: &str) -> Vec<String> {
    let mut zones = supported_timezones();
    for extra in [Some(current.to_string()), browser_timezone()]
        .into_iter()
        .flatten()
    {
        if !extra.is_empty() && !zones.contains(&extra) {
            zones.push(extra);
        }
    }
    zones.sort();
    zones
}

/// `Tue 14:05` for an epoch-ms instant in `tz` (browser zone when `None` or invalid).
pub fn format_deadline(ms: f64, tz: Option<&str>) -> String {
    let date = Date::new(&JsValue::from_f64(ms));
    let Some(f) = get(&date, "toLocaleString").and_then(|f| f.dyn_into::<Function>().ok()) else {
        return String::new();
    };
    let build = |zone: Option<&str>| {
        let o = Object::new();
        for (k, v) in [
            ("weekday", "short"),
            ("hour", "2-digit"),
            ("minute", "2-digit"),
            ("hourCycle", "h23"),
        ] {
            let _ = Reflect::set(&o, &k.into(), &v.into());
        }
        if let Some(z) = zone {
            let _ = Reflect::set(&o, &"timeZone".into(), &z.into());
        }
        o
    };
    let locale = JsValue::from_str("en-US");
    let call = |zone| f.call2(&date, &locale, &build(zone)).ok()?.as_string();
    tz.filter(|t| !t.is_empty())
        .and_then(|t| call(Some(t)))
        .or_else(|| call(None))
        .unwrap_or_default()
        .replace(',', "")
}

/// The signed-in user's zone (falls back to the browser's).
pub fn my_timezone() -> Option<String> {
    use_context::<AuthState>()
        .and_then(|a| {
            a.user
                .with_untracked(|u| u.as_ref().and_then(|u| u.timezone.clone()))
        })
        .filter(|t| !t.is_empty())
        .or_else(browser_timezone)
}

/// "Tue 14:05 your time" for a deadline.
pub fn deadline_text(ms: f64) -> String {
    format!(
        "{} your time",
        format_deadline(ms, my_timezone().as_deref())
    )
}

/// `<select>` of IANA zones bound to `value`; an empty value shows the browser's zone.
#[component]
pub fn TimezoneSelect(value: RwSignal<String>) -> impl IntoView {
    let zones = zone_options(&value.get_untracked());
    let browser = browser_timezone();
    let shown = move || {
        let v = value.get();
        if v.is_empty() {
            browser.clone().unwrap_or_else(|| "UTC".into())
        } else {
            v
        }
    };
    view! {
        <label>"Time zone"
            <select aria-label="Time zone" on:change:target=move |ev| value.set(ev.target().value())>
                {zones.into_iter().map(|z| {
                    let z2 = z.clone();
                    let shown = shown.clone();
                    view! { <option value=z selected=move || shown() == z2>{z2.clone()}</option> }
                }).collect_view()}
            </select>
        </label>
    }
}

/// Asks once after login if the account has no time zone yet.
#[component]
pub fn TimezonePrompt() -> impl IntoView {
    let auth = AuthState::expect();
    let toasts = crate::state::Toasts::expect();
    let open = RwSignal::new(false);
    let dismissed = RwSignal::new(false);
    let zone = RwSignal::new(browser_timezone().unwrap_or_else(|| "UTC".into()));
    let show = Memo::new(move |_| {
        auth.user
            .with(|u| u.as_ref().is_some_and(|u| u.timezone.is_none()))
            && !dismissed.get()
    });
    Effect::new(move |_| open.set(show.get()));
    // closing the dialog by any route (Not now, Close, backdrop) counts as dismissed
    Effect::new(move |prev: Option<bool>| {
        let o = open.get();
        if prev == Some(true) && !o {
            dismissed.set(true);
        }
        o
    });
    let save = move |_| {
        let req = wartide_protocol::user::UpdateProfileRequest {
            timezone: Some(zone.get_untracked()),
            ..Default::default()
        };
        leptos::task::spawn_local(async move {
            match crate::api::update_profile(&req).await {
                Ok(_) => {
                    auth.refresh().await;
                    toasts.info("Time zone saved");
                }
                Err(e) => toasts.api_error(&e),
            }
        });
    };
    view! {
        <crate::components::modal::Modal open title="Your time zone">
            <div class="col">
                <p class="muted">"Turn deadlines are shown in your local time. You can change this later in Settings."</p>
                <TimezoneSelect value=zone />
                <div class="row">
                    <button on:click=save>"Save"</button>
                    <button class="secondary" on:click=move |_| open.set(false)>"Not now"</button>
                </div>
            </div>
        </crate::components::modal::Modal>
    }
}
