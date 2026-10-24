use crate::api;
use crate::components::form::{Check, TextField};
use crate::state::{AuthState, Toasts};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use wartide_protocol::user::{UpdateProfileRequest, UpdateSettingsRequest};

#[component]
pub fn Settings() -> impl IntoView {
    let auth = AuthState::expect();
    let toasts = Toasts::expect();
    let display = RwSignal::new(String::new());
    let bio = RwSignal::new(String::new());
    let country = RwSignal::new(String::new());
    let timezone = RwSignal::new(String::new());
    let notify = RwSignal::new(true);
    let loaded = RwSignal::new(false);

    spawn_local(async move {
        if let Some(name) = auth.username() {
            match api::profile(&name).await {
                Ok(p) => {
                    display.set(p.display_name);
                    bio.set(p.bio.unwrap_or_default());
                    country.set(p.country.unwrap_or_default());
                    timezone.set(p.timezone.unwrap_or_default());
                }
                Err(e) => toasts.api_error(&e),
            }
        }
        match api::settings().await {
            Ok(s) => notify.set(s.email_turn_notifications),
            Err(e) => toasts.api_error(&e),
        }
        loaded.set(true);
    });

    let save = move |_| {
        let opt = |s: String| Some(s.trim().to_string());
        let req = UpdateProfileRequest {
            display_name: opt(display.get_untracked()).filter(|s| !s.is_empty()),
            avatar: None,
            bio: opt(bio.get_untracked()),
            country: opt(country.get_untracked()),
            timezone: opt(timezone.get_untracked()),
        };
        spawn_local(async move {
            let s = api::update_settings(&UpdateSettingsRequest {
                email_turn_notifications: Some(notify.get_untracked()),
                ..Default::default()
            })
            .await;
            match (api::update_profile(&req).await, s) {
                (Ok(_), Ok(_)) => {
                    auth.refresh().await;
                    toasts.info("Settings saved");
                }
                (Err(e), _) | (_, Err(e)) => toasts.api_error(&e),
            }
        });
    };
    view! {
        <div class="page grid2">
            <form class="card col" on:submit=|ev| ev.prevent_default()>
                <h2>"Profile"</h2>
                <TextField label="Display name" value=display />
                <label>"Bio"
                    <textarea rows="3" prop:value=move || bio.get() on:input:target=move |ev| bio.set(ev.target().value())></textarea>
                </label>
                <TextField label="Country" value=country />
                <TextField label="Time zone" value=timezone placeholder="Europe/Berlin" />
                <Check label="Email me when it is my turn" value=notify />
                <button type="submit" disabled=move || !loaded.get() on:click=save>"Save"</button>
            </form>
            <div class="card col">
                <h2>"Password"</h2>
                <p class="muted">"To change your password, request a reset link by email."</p>
                <A href="/forgot"><span class="btn">"Send me a reset link"</span></A>
            </div>
        </div>
    }
}
