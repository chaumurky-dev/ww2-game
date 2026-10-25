use crate::api;
use crate::components::form::{Check, TextField};
use crate::components::modal::Modal;
use crate::state::{AuthState, Toasts};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use wartide_protocol::user::{
    CasualtyPolicy, SubWithdrawalPolicy, UpdateProfileRequest, UpdateSettingsRequest,
};

#[component]
pub fn Settings() -> impl IntoView {
    let auth = AuthState::expect();
    let toasts = Toasts::expect();
    let display = RwSignal::new(String::new());
    let bio = RwSignal::new(String::new());
    let country = RwSignal::new(String::new());
    let timezone = RwSignal::new(String::new());
    let notify = RwSignal::new(true);
    let match_mail = RwSignal::new(true);
    let casualty = RwSignal::new(CasualtyPolicy::default());
    let sub_wd = RwSignal::new(SubWithdrawalPolicy::default());
    let cur_pw = RwSignal::new(String::new());
    let new_pw = RwSignal::new(String::new());
    let new_pw2 = RwSignal::new(String::new());
    let pw_busy = RwSignal::new(false);
    let del_open = RwSignal::new(false);
    let del_text = RwSignal::new(String::new());
    let del_pw = RwSignal::new(String::new());
    let navigate = use_navigate();
    let loaded = RwSignal::new(false);

    spawn_local(async move {
        if let Some(name) = auth.username() {
            match api::profile(&name).await {
                Ok(p) => {
                    display.set(p.display_name);
                    bio.set(p.bio.unwrap_or_default());
                    country.set(p.country.unwrap_or_default());
                    timezone.set(
                        p.timezone
                            .or_else(crate::tz::browser_timezone)
                            .unwrap_or_default(),
                    );
                }
                Err(e) => toasts.api_error(&e),
            }
        }
        match api::settings().await {
            Ok(s) => {
                notify.set(s.email_turn_notifications);
                match_mail.set(s.email_match_found);
                casualty.set(s.casualty_policy);
                sub_wd.set(s.sub_withdrawal);
            }
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
                email_match_found: Some(match_mail.get_untracked()),
                casualty_policy: Some(casualty.get_untracked()),
                sub_withdrawal: Some(sub_wd.get_untracked()),
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
    let change_pw = move |_| {
        if new_pw.get_untracked() != new_pw2.get_untracked() {
            toasts.error("The new passwords do not match.");
            return;
        }
        if new_pw.get_untracked().chars().count() < 8 {
            toasts.error("The new password needs at least 8 characters.");
            return;
        }
        pw_busy.set(true);
        spawn_local(async move {
            match api::change_password(&cur_pw.get_untracked(), &new_pw.get_untracked()).await {
                Ok(()) => {
                    toasts.info("Password changed. Other devices were signed out.");
                    cur_pw.set(String::new());
                    new_pw.set(String::new());
                    new_pw2.set(String::new());
                }
                Err(e) => toasts.api_error(&e),
            }
            pw_busy.set(false);
        });
    };
    let delete = Callback::new(move |_: ()| {
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::delete_account(&del_pw.get_untracked()).await {
                Ok(()) => {
                    del_open.set(false);
                    auth.set(None);
                    toasts.info("Your account was deleted.");
                    navigate("/", Default::default());
                }
                Err(e) => toasts.api_error(&e),
            }
        });
    });
    view! {
        <div class="page grid2">
            <form class="card col" on:submit=|ev| ev.prevent_default()>
                <h2>"Profile"</h2>
                <TextField label="Display name" value=display />
                <label>"Bio"
                    <textarea rows="3" prop:value=move || bio.get() on:input:target=move |ev| bio.set(ev.target().value())></textarea>
                </label>
                <TextField label="Country" value=country />
                <crate::tz::TimezoneSelect value=timezone />
                <Check label="Email me when it is my turn" value=notify />
                <Check label="Email me when a matchmaking game is found" value=match_mail />
                <h2>"Battles"</h2>
                <p class="muted small-text">"When you defend, nobody waits for you: the server picks your casualties and decides your submarine withdrawals using these preferences."</p>
                <label>"Casualties when defending"
                    <select aria-label="Casualty policy" on:change:target=move |ev| {
                        if let Some(p) = CasualtyPolicy::ALL.iter().find(|p| p.id() == ev.target().value()) {
                            casualty.set(*p);
                        }
                    }>
                        {CasualtyPolicy::ALL.iter().map(|&p| view! {
                            <option value=p.id() selected=move || casualty.get() == p>{p.label()}</option>
                        }).collect_view()}
                    </select>
                </label>
                <p class="muted small-text">{move || casualty.get().describe()}</p>
                <label>"Submarines when defending"
                    <select aria-label="Submarine withdrawal" on:change:target=move |ev| {
                        if let Some(p) = SubWithdrawalPolicy::ALL.iter().find(|p| p.id() == ev.target().value()) {
                            sub_wd.set(*p);
                        }
                    }>
                        {SubWithdrawalPolicy::ALL.iter().map(|&p| view! {
                            <option value=p.id() selected=move || sub_wd.get() == p>{p.label()}</option>
                        }).collect_view()}
                    </select>
                </label>
                <p class="muted small-text">{move || sub_wd.get().describe()}</p>
                <button type="submit" disabled=move || !loaded.get() on:click=save>"Save"</button>
            </form>
            <div class="col">
                <form class="card col" on:submit=|ev| ev.prevent_default()>
                    <h2>"Change password"</h2>
                    <TextField label="Current password" value=cur_pw kind="password" />
                    <TextField label="New password (8+ characters)" value=new_pw kind="password" />
                    <TextField label="Repeat new password" value=new_pw2 kind="password" />
                    <button type="submit" disabled=move || pw_busy.get() || cur_pw.with(|c| c.is_empty()) on:click=change_pw>"Change password"</button>
                    <p class="muted small-text">"Other devices are signed out; this one stays logged in."</p>
                </form>
                <div class="card col">
                    <h2>"Delete account"</h2>
                    <p class="muted">"Your name and e-mail are scrubbed. Games you played stay in the history under \"Deleted user\"."</p>
                    <button class="danger" on:click=move |_| { del_text.set(String::new()); del_pw.set(String::new()); del_open.set(true); }>"Delete my account"</button>
                </div>
            </div>
            <Modal open=del_open title="Delete your account?">
                <div class="col">
                    <p>"This cannot be undone. You will be logged out everywhere and your profile disappears from the leaderboards."</p>
                    <TextField label="Type DELETE to confirm" value=del_text />
                    <TextField label="Your password" value=del_pw kind="password" />
                    <div class="row">
                        <button class="danger" disabled=move || del_text.with(|t| t.trim() != "DELETE") || del_pw.with(|p| p.is_empty()) on:click=move |_| delete.run(())>"Delete forever"</button>
                        <button class="secondary" on:click=move |_| del_open.set(false)>"Cancel"</button>
                    </div>
                </div>
            </Modal>
        </div>
    }
}
