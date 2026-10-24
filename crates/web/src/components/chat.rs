use crate::mock::{Channel, ChatMsg, LogEntry, Power};
use leptos::prelude::*;

/// Chat drawer with All / Team / Power tabs. TODO(protocol): send via POST + WS push.
#[component]
pub fn ChatDrawer(messages: RwSignal<Vec<ChatMsg>>, me: String, my_power: Power) -> impl IntoView {
    let tab = RwSignal::new(Channel::All);
    let draft = RwSignal::new(String::new());
    let send = {
        let me = me.clone();
        move || {
            let text = draft.get_untracked();
            if text.trim().is_empty() {
                return;
            }
            let msg = ChatMsg { channel: tab.get_untracked(), author: me.clone(), power: Some(my_power), bot: false, text };
            messages.update(|m| m.push(msg));
            draft.set(String::new());
        }
    };
    let send2 = send.clone();
    let tab_btn = move |c: Channel, label: &'static str| {
        view! { <span class="tab" class:on=move || tab.get() == c on:click=move |_| tab.set(c)>{label}</span> }
    };
    view! {
        <div class="chat">
            <div class="tabs">
                {tab_btn(Channel::All, "All")} {tab_btn(Channel::Team, "Team")} {tab_btn(Channel::Power, "Power")}
            </div>
            <div class="msgs">
                {move || {
                    messages
                        .get()
                        .into_iter()
                        .filter(|m| m.channel == tab.get())
                        .map(|m| {
                            let color = m.power.map(|p| p.color()).unwrap_or("#888");
                            view! {
                                <div class=if m.bot { "msg bot" } else { "msg" } style=format!("--pc:{color}")>
                                    <strong>{m.author}</strong>
                                    {if m.bot { " [bot]" } else { "" }}
                                    ": " {m.text}
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
            <div class="row">
                <input
                    class="grow"
                    placeholder="Say something..."
                    prop:value=move || draft.get()
                    on:input:target=move |ev| draft.set(ev.target().value())
                    on:keydown=move |ev| if ev.key() == "Enter" { send2() }
                />
                <button class="small" on:click=move |_| send()>"Send"</button>
            </div>
        </div>
    }
}

#[component]
pub fn ActionLog(entries: RwSignal<Vec<LogEntry>>) -> impl IntoView {
    view! {
        <div>
            <h3>"Log"</h3>
            <div class="log">
                {move || {
                    entries
                        .get()
                        .into_iter()
                        .rev()
                        .map(|e| view! { <div><span class="muted">{format!("R{} ", e.round)}</span>{e.text}</div> })
                        .collect_view()
                }}
            </div>
        </div>
    }
}
