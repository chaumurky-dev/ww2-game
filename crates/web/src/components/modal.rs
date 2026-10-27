use leptos::prelude::*;
use wasm_bindgen::JsCast;

const FOCUSABLE: &str = "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";

fn active() -> Option<web_sys::HtmlElement> {
    web_sys::window()?
        .document()?
        .active_element()?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()
}

fn focusables(dialog: &web_sys::Element) -> Vec<web_sys::HtmlElement> {
    let Ok(list) = dialog.query_selector_all(FOCUSABLE) else {
        return vec![];
    };
    (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        .collect()
}

/// Keyboard handling of an open dialog: Escape closes, Tab / Shift+Tab cycle inside it.
fn on_key(open: RwSignal<bool>, ev: web_sys::KeyboardEvent) {
    match ev.key().as_str() {
        "Escape" => {
            ev.prevent_default();
            ev.stop_propagation();
            open.set(false);
        }
        "Enter" => {
            // Enter confirms via the `data-autofocus` button unless a button already has focus
            // (then the browser activates that one).
            let on_button = active().is_some_and(|a| a.tag_name() == "BUTTON");
            if on_button {
                return;
            }
            let Some(dialog) = ev
                .current_target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            else {
                return;
            };
            if let Some(b) = dialog
                .query_selector("[data-autofocus]")
                .ok()
                .flatten()
                .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
            {
                ev.prevent_default();
                b.click();
            }
        }
        "Tab" => {
            let Some(dialog) = ev
                .current_target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            else {
                return;
            };
            let items = focusables(&dialog);
            let (Some(first), Some(last)) = (items.first(), items.last()) else {
                ev.prevent_default();
                return;
            };
            let cur = active();
            let inside = cur.as_ref().is_some_and(|c| dialog.contains(Some(c)));
            if !inside || (ev.shift_key() && cur.as_ref() == Some(first)) {
                ev.prevent_default();
                let _ = if inside { last.focus() } else { first.focus() };
            } else if !ev.shift_key() && cur.as_ref() == Some(last) {
                ev.prevent_default();
                let _ = first.focus();
            }
        }
        _ => {}
    }
}

/// Simple modal dialog; closes on backdrop click, Escape or the close button. Focus moves into
/// the dialog on open (to the `data-autofocus` element if any), stays there while Tab cycles,
/// and returns to the previously focused element on close.
#[component]
pub fn Modal(
    open: RwSignal<bool>,
    #[prop(into)] title: String,
    children: ChildrenFn,
) -> impl IntoView {
    let title = StoredValue::new(title);
    let dialog = NodeRef::<leptos::html::Div>::new();
    let prev = StoredValue::new_local(None::<web_sys::HtmlElement>);
    Effect::new(move |was: Option<bool>| {
        let now = open.get();
        if now && was != Some(true) {
            prev.set_value(active());
            // The dialog is rendered by `Show` after this effect; wait a tick.
            gloo_timers::callback::Timeout::new(0, move || {
                if let Some(d) = dialog.get_untracked() {
                    let target = d
                        .query_selector("[data-autofocus]")
                        .ok()
                        .flatten()
                        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
                        .or_else(|| focusables(&d).into_iter().next());
                    if let Some(t) = target {
                        let _ = t.focus();
                    }
                }
            })
            .forget();
        } else if !now && was == Some(true) {
            if let Some(p) = prev.get_value() {
                let _ = p.focus();
            }
        }
        now
    });
    view! {
        <Show when=move || open.get()>
            <div class="modal-back" on:click=move |_| open.set(false)>
                <div class="modal" role="dialog" aria-modal="true" node_ref=dialog tabindex="-1"
                    on:click=|ev| ev.stop_propagation()
                    on:keydown=move |ev| on_key(open, ev)>
                    <div class="row">
                        <h2 class="grow">{move || title.get_value()}</h2>
                        <button class="secondary small" on:click=move |_| open.set(false)>"Close"</button>
                    </div>
                    {children()}
                </div>
            </div>
        </Show>
    }
}
