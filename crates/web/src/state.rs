//! App-wide context: auth state and toast notifications.
use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use wartide_protocol::auth::SessionUser;
use wartide_protocol::ApiError;

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn store_get(key: &str) -> Option<String> {
    storage()?.get_item(key).ok().flatten()
}

pub fn store_set(key: &str, value: Option<&str>) {
    if let Some(s) = storage() {
        let _ = match value {
            Some(v) => s.set_item(key, v),
            None => s.remove_item(key),
        };
    }
}

/// Global auth resource, filled from `GET /api/me` at start-up.
#[derive(Clone, Copy)]
pub struct AuthState {
    pub user: RwSignal<Option<SessionUser>>,
    /// False until the first `/api/me` answered.
    pub loaded: RwSignal<bool>,
}

impl AuthState {
    pub fn provide() -> Self {
        let s = AuthState {
            user: RwSignal::new(None),
            loaded: RwSignal::new(false),
        };
        provide_context(s);
        s
    }
    pub fn expect() -> Self {
        expect_context::<AuthState>()
    }
    pub fn set(&self, user: Option<SessionUser>) {
        self.user.set(user);
        self.loaded.set(true);
    }
    /// Re-read `/api/me` (after login, email verification, ...).
    pub async fn refresh(&self) {
        match crate::api::me().await {
            Ok(u) => self.set(Some(u)),
            Err(_) => self.set(None),
        }
    }
    pub fn username(&self) -> Option<String> {
        self.user.with(|u| u.as_ref().map(|u| u.username.clone()))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    pub id: u32,
    pub kind: ToastKind,
    pub text: String,
    pub href: Option<String>,
}

#[derive(Clone, Copy)]
pub struct Toasts {
    pub items: RwSignal<Vec<Toast>>,
    next: RwSignal<u32>,
}

impl Toasts {
    pub fn provide() -> Self {
        let t = Toasts {
            items: RwSignal::new(vec![]),
            next: RwSignal::new(0),
        };
        provide_context(t);
        t
    }
    pub fn expect() -> Self {
        expect_context::<Toasts>()
    }
    fn push_full(&self, kind: ToastKind, text: String, href: Option<String>, ms: u32) {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        self.items.update(|v| {
            v.push(Toast {
                id,
                kind,
                text,
                href,
            })
        });
        let items = self.items;
        Timeout::new(ms, move || items.update(|v| v.retain(|t| t.id != id))).forget();
    }
    pub fn push(&self, kind: ToastKind, text: impl Into<String>) {
        self.push_full(kind, text.into(), None, 4500);
    }
    pub fn info(&self, text: impl Into<String>) {
        self.push(ToastKind::Info, text);
    }
    pub fn error(&self, text: impl Into<String>) {
        self.push(ToastKind::Error, text);
    }
    /// A short informational toast (`ms` milliseconds).
    pub fn short(&self, text: impl Into<String>, ms: u32) {
        self.push_full(ToastKind::Info, text.into(), None, ms);
    }
    /// A longer-lived toast with a link (e.g. "your turn" / "match found").
    pub fn link(&self, text: impl Into<String>, href: impl Into<String>) {
        self.push_full(ToastKind::Warn, text.into(), Some(href.into()), 12000);
    }
    /// Show an [`ApiError`]; an `unauthorized` answer also clears the session.
    pub fn api_error(&self, e: &ApiError) {
        if e.code == "unauthorized" {
            if let Some(auth) = use_context::<AuthState>() {
                auth.user.set(None);
            }
        }
        self.error(e.message.clone());
    }
}
