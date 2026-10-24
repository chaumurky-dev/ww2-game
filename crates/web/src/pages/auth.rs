use crate::api;
use crate::components::form::TextField;
use crate::state::AuthState;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

fn token() -> String {
    use_query_map().with_untracked(|q| q.get("token").unwrap_or_default())
}

#[component]
pub fn Verify() -> impl IntoView {
    let t = token();
    let auth = AuthState::expect();
    let status = LocalResource::new(move || {
        let t = t.clone();
        async move {
            let r = api::verify_email(&t).await;
            if r.is_ok() && auth.user.get_untracked().is_some() {
                auth.refresh().await;
            }
            r
        }
    });
    view! {
        <div class="page"><div class="card">
            <h2>"Email verification"</h2>
            <Suspense fallback=|| view! { <p>"Verifying..."</p> }>
                {move || status.get().map(|r| match r {
                    Ok(()) => view! { <p>"Your email is verified. " <A href="/">"Log in"</A></p> }.into_any(),
                    Err(e) => view! { <p class="error">{e.message}</p> }.into_any(),
                })}
            </Suspense>
        </div></div>
    }
}

#[component]
pub fn Forgot() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let msg = RwSignal::new(None::<Result<String, String>>);
    let go = move |_| {
        spawn_local(async move {
            let r = api::forgot_password(&email.get_untracked()).await;
            msg.set(Some(match r {
                Ok(()) => Ok("If that address exists, a reset link is on its way.".into()),
                Err(e) => Err(e.message),
            }));
        });
    };
    view! {
        <div class="page"><form class="card col" on:submit=|ev| ev.prevent_default()>
            <h2>"Forgot password"</h2>
            <TextField label="Email" value=email kind="email" />
            <button type="submit" on:click=go>"Send reset link"</button>
            {move || msg.get().map(|r| match r {
                Ok(m) => view! { <p>{m}</p> }.into_any(),
                Err(e) => view! { <p class="error">{e}</p> }.into_any(),
            })}
        </form></div>
    }
}

#[component]
pub fn Reset() -> impl IntoView {
    let t = token();
    let pw = RwSignal::new(String::new());
    let msg = RwSignal::new(None::<Result<String, String>>);
    let go = move |_| {
        let t = t.clone();
        spawn_local(async move {
            let r = api::reset_password(&t, &pw.get_untracked()).await;
            msg.set(Some(match r {
                Ok(()) => Ok("Password updated. You can log in now.".into()),
                Err(e) => Err(e.message),
            }));
        });
    };
    view! {
        <div class="page"><form class="card col" on:submit=|ev| ev.prevent_default()>
            <h2>"Reset password"</h2>
            <TextField label="New password" value=pw kind="password" />
            <button type="submit" on:click=go>"Set password"</button>
            {move || msg.get().map(|r| match r {
                Ok(m) => view! { <p>{m} " " <A href="/">"Log in"</A></p> }.into_any(),
                Err(e) => view! { <p class="error">{e}</p> }.into_any(),
            })}
        </form></div>
    }
}
