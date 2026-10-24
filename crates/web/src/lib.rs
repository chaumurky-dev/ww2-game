//! Wartide web client shell (Leptos 0.8, CSR).
pub mod api;
pub mod components;
pub mod game;
pub mod map;
pub mod model;
pub mod pages;
pub mod realtime;
pub mod state;

use components::guard::Protected;
use components::nav::NavBar;
use components::toast::ToastHost;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use map::SharedMap;
use pages::*;
use state::{AuthState, Toasts};

/// Map geometry + engine map data, fetched once and shared by every page that draws the board.
#[derive(Clone, Copy)]
pub struct MapCtx(pub LocalResource<Result<SharedMap, String>>);

#[component]
pub fn App() -> impl IntoView {
    let auth = AuthState::provide();
    Toasts::provide();
    realtime::Realtime::provide();
    provide_context(MapCtx(LocalResource::new(map::load)));
    spawn_local(async move { auth.refresh().await });
    view! {
        <Router>
            <NavBar />
            <main>
                <Routes fallback=|| view! { <div class="page"><h2>"Not found"</h2></div> }>
                    <Route path=path!("/") view=landing::Landing />
                    <Route path=path!("/verify") view=auth::Verify />
                    <Route path=path!("/forgot") view=auth::Forgot />
                    <Route path=path!("/reset") view=auth::Reset />
                    <Route path=path!("/lobby") view=|| view! { <Protected><lobby::Lobby /></Protected> } />
                    <Route path=path!("/game/:id") view=|| view! { <Protected><pages::game::GamePage /></Protected> } />
                    <Route path=path!("/sandbox") view=game::sandbox::Sandbox />
                    <Route path=path!("/profile/:username") view=profile::ProfilePage />
                    <Route path=path!("/groups") view=|| view! { <Protected><groups::GroupList /></Protected> } />
                    <Route path=path!("/groups/:slug") view=|| view! { <Protected><groups::GroupDetail /></Protected> } />
                    <Route path=path!("/leaderboard") view=leaderboard::Leaderboard />
                    <Route path=path!("/settings") view=|| view! { <Protected><settings::Settings /></Protected> } />
                </Routes>
            </main>
            <ToastHost />
        </Router>
    }
}

pub fn run() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
