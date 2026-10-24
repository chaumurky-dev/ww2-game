use leptos::prelude::*;

/// Simple modal dialog; closes on backdrop click or the close button.
#[component]
pub fn Modal(
    open: RwSignal<bool>,
    #[prop(into)] title: String,
    children: ChildrenFn,
) -> impl IntoView {
    let title = StoredValue::new(title);
    view! {
        <Show when=move || open.get()>
            <div class="modal-back" on:click=move |_| open.set(false)>
                <div class="modal" role="dialog" aria-modal="true" on:click=|ev| ev.stop_propagation()>
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
