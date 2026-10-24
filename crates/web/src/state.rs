//! App-wide context: session and toast notifications.
use crate::mock::User;
use gloo_timers::callback::Timeout;
use leptos::prelude::*;

const USER_KEY: &str = "wartide.user";

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

#[derive(Clone, Copy)]
pub struct Session {
    pub user: RwSignal<Option<User>>,
}

impl Session {
    pub fn provide() -> Self {
        // TODO(protocol): restore the session from GET /api/auth/me (cookie/JWT) instead.
        let restored = store_get(USER_KEY).map(|name| crate::mock::user_named(&name));
        let s = Session {
            user: RwSignal::new(restored),
        };
        provide_context(s);
        s
    }
    pub fn expect() -> Self {
        expect_context::<Session>()
    }
    pub fn login(&self, user: User) {
        store_set(USER_KEY, Some(&user.username));
        self.user.set(Some(user));
    }
    pub fn logout(&self) {
        store_set(USER_KEY, None);
        self.user.set(None);
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
    pub fn push(&self, kind: ToastKind, text: impl Into<String>) {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        self.items.update(|v| {
            v.push(Toast {
                id,
                kind,
                text: text.into(),
            })
        });
        let items = self.items;
        Timeout::new(4500, move || items.update(|v| v.retain(|t| t.id != id))).forget();
    }
    pub fn info(&self, text: impl Into<String>) {
        self.push(ToastKind::Info, text);
    }
    pub fn error(&self, text: impl Into<String>) {
        self.push(ToastKind::Error, text);
    }
}
