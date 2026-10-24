use crate::api;
use crate::components::form::TextField;
use crate::state::{Session, Toasts};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

#[component]
pub fn Landing() -> impl IntoView {
    let session = Session::expect();
    let toasts = Toasts::expect();
    let navigate = use_navigate();
    let (lu, lp) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let (ru, re, rp) = (RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let do_login = {
        let navigate = navigate.clone();
        move |_| {
            busy.set(true);
            let navigate = navigate.clone();
            spawn_local(async move {
                match api::login(&lu.get_untracked(), &lp.get_untracked()).await {
                    Ok(u) => {
                        session.login(u);
                        navigate("/lobby", Default::default());
                    }
                    Err(e) => error.set(Some(e)),
                }
                busy.set(false);
            });
        }
    };
    let do_register = move |_| {
        spawn_local(async move {
            match api::register(&ru.get_untracked(), &re.get_untracked(), &rp.get_untracked()).await {
                Ok(()) => toasts.info("Account created. Check your email for the verification link."),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <div class="page">
            <div class="hero">
                <h1>"WARTIDE"</h1>
                <p class="serif">
                    "Five great powers. One world at war. Play by web, at your own pace, against friends, strangers, or a chatty bot general."
                </p>
            </div>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            <Show
                when=move || session.user.get().is_none()
                fallback=move || view! { <div class="card"><A href="/lobby"><span class="btn">"Go to the lobby"</span></A></div> }
            >
                <div class="grid2">
                    <form class="card col" on:submit=|ev| ev.prevent_default()>
                        <h2>"Log in"</h2>
                        <TextField label="Username" value=lu />
                        <TextField label="Password" value=lp kind="password" />
                        <div class="row">
                            <button disabled=move || busy.get() on:click=do_login.clone()>"Log in"</button>
                            <A href="/forgot">"Forgot password?"</A>
                        </div>
                    </form>
                    <form class="card col" on:submit=|ev| ev.prevent_default()>
                        <h2>"Enlist"</h2>
                        <TextField label="Username" value=ru />
                        <TextField label="Email" value=re kind="email" />
                        <TextField label="Password" value=rp kind="password" />
                        <button on:click=do_register>"Create account"</button>
                    </form>
                </div>
            </Show>
        </div>
    }
}
