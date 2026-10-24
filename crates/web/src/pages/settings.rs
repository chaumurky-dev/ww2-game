use crate::api;
use crate::components::form::{Check, TextField};
use crate::state::{Session, Toasts};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn Settings() -> impl IntoView {
    let session = Session::expect();
    let toasts = Toasts::expect();
    let Some(user) = session.user.get_untracked() else {
        return view! { <div class="page"><p>"Please log in to edit settings."</p></div> }
            .into_any();
    };
    let display = RwSignal::new(user.display_name.clone());
    let bio = RwSignal::new(user.bio.clone());
    let notify = RwSignal::new(true);
    let (old, new) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let save = {
        let user = user.clone();
        move |_| {
            let mut u = user.clone();
            u.display_name = display.get_untracked();
            u.bio = bio.get_untracked();
            spawn_local(async move {
                match api::save_settings(&u, notify.get_untracked()).await {
                    Ok(()) => {
                        session.user.set(Some(u));
                        toasts.info("Settings saved");
                    }
                    Err(e) => toasts.error(e),
                }
            });
        }
    };
    let change = move |_| {
        spawn_local(async move {
            match api::change_password(&old.get_untracked(), &new.get_untracked()).await {
                Ok(()) => toasts.info("Password changed"),
                Err(e) => toasts.error(e),
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
                <Check label="Email me when it is my turn" value=notify />
                <button on:click=save>"Save"</button>
            </form>
            <form class="card col" on:submit=|ev| ev.prevent_default()>
                <h2>"Password"</h2>
                <TextField label="Current password" value=old kind="password" />
                <TextField label="New password" value=new kind="password" />
                <button on:click=change>"Change password"</button>
            </form>
        </div>
    }
    .into_any()
}
