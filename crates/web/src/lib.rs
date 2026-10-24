//! Wartide web client shell (Leptos 0.8, CSR).
pub mod api;
pub mod components;
pub mod geometry;
pub mod mock;
pub mod pages;
pub mod state;

use components::nav::NavBar;
use components::toast::ToastHost;
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use pages::*;
use state::{Session, Toasts};

#[component]
pub fn App() -> impl IntoView {
    Session::provide();
    Toasts::provide();
    view! {
        <Router>
            <NavBar />
            <main>
                <Routes fallback=|| view! { <div class="page"><h2>"Not found"</h2></div> }>
                    <Route path=path!("/") view=landing::Landing />
                    <Route path=path!("/verify") view=auth::Verify />
                    <Route path=path!("/forgot") view=auth::Forgot />
                    <Route path=path!("/reset") view=auth::Reset />
                    <Route path=path!("/lobby") view=lobby::Lobby />
                    <Route path=path!("/game/:id") view=game::GamePage />
                    <Route path=path!("/profile/:username") view=profile::ProfilePage />
                    <Route path=path!("/groups") view=groups::GroupList />
                    <Route path=path!("/groups/:slug") view=groups::GroupDetail />
                    <Route path=path!("/leaderboard") view=leaderboard::Leaderboard />
                    <Route path=path!("/settings") view=settings::Settings />
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
