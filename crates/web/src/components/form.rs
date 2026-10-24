use leptos::prelude::*;

#[component]
pub fn TextField(
    #[prop(into)] label: String,
    value: RwSignal<String>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(optional, into)] placeholder: String,
) -> impl IntoView {
    view! {
        <label>
            {label}
            <input
                type=kind
                placeholder=placeholder
                prop:value=move || value.get()
                on:input:target=move |ev| value.set(ev.target().value())
            />
        </label>
    }
}

#[component]
pub fn Check(#[prop(into)] label: String, value: RwSignal<bool>) -> impl IntoView {
    view! {
        <label class="check">
            <input type="checkbox" prop:checked=move || value.get() on:change:target=move |ev| value.set(ev.target().checked()) />
            {label}
        </label>
    }
}
