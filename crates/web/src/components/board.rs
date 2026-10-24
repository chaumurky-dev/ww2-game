//! SVG game board with pan/zoom. Decoupled from the engine through [`BoardModel`].
use super::unit_icon::UnitBadge;
use super::viewport::ViewBox;
use crate::geometry::{Geometry, LocKind};
use crate::mock::{Power, UnitKind};
use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

/// A stack of identical units at a location.
#[derive(Debug, Clone, PartialEq)]
pub struct Stack {
    pub power: Power,
    pub kind: UnitKind,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    Move,
    Attack,
    Battle,
}

/// View-model for the board; to be filled from the engine's `GameState`.
/// Keys are geometry location ids.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardModel {
    pub owners: HashMap<String, Power>,
    pub stacks: HashMap<String, Vec<Stack>>,
    pub highlights: HashMap<String, Highlight>,
    pub selected: Option<String>,
}

fn loc_class(kind: LocKind, sea_alt: bool, id: &str, m: &BoardModel) -> String {
    let mut c = String::from("loc");
    match kind {
        LocKind::Sea => c.push_str(if sea_alt { " sea-b" } else { " sea-a" }),
        LocKind::Land => {
            c.push_str(" land");
            if let Some(p) = m.owners.get(id) {
                c.push_str(&format!(" owner-{}", p.id()));
            }
        }
    }
    match m.highlights.get(id) {
        Some(Highlight::Move) => c.push_str(" hl-move"),
        Some(Highlight::Attack) => c.push_str(" hl-attack"),
        Some(Highlight::Battle) => c.push_str(" hl-battle"),
        None => {}
    }
    if m.selected.as_deref() == Some(id) {
        c.push_str(" selected");
    }
    c
}

fn rect_of(target: Option<web_sys::EventTarget>) -> Option<(f64, f64, f64, f64)> {
    let el = target?.dyn_into::<web_sys::Element>().ok()?;
    let r = el.get_bounding_client_rect();
    Some((r.left(), r.top(), r.width(), r.height()))
}

type Pointers = Vec<(i32, f64, f64)>;

#[component]
pub fn Board(
    geometry: Geometry,
    #[prop(into)] model: Signal<BoardModel>,
    on_select: Callback<Option<String>>,
) -> impl IntoView {
    let base = ViewBox::new(geometry.view_box);
    let view = RwSignal::new(base);
    let hover = RwSignal::new(None::<String>);
    let dragging = RwSignal::new(false);
    let pts = StoredValue::new(Pointers::new());
    let moved = StoredValue::new(0.0_f64);
    let size = StoredValue::new((800.0_f64, 600.0_f64));

    let on_down = move |ev: web_sys::PointerEvent| {
        if let Some((_, _, w, h)) = rect_of(ev.current_target()) {
            size.set_value((w, h));
        }
        pts.update_value(|p| {
            if p.is_empty() {
                moved.set_value(0.0);
            }
            p.push((ev.pointer_id(), ev.client_x() as f64, ev.client_y() as f64));
        });
    };
    let on_move = move |ev: web_sys::PointerEvent| {
        let (x, y, id) = (ev.client_x() as f64, ev.client_y() as f64, ev.pointer_id());
        let old = pts.get_value();
        let Some(i) = old.iter().position(|p| p.0 == id) else {
            return;
        };
        let (cw, ch) = size.get_value();
        match old.len() {
            1 => {
                let (dx, dy) = (x - old[0].1, y - old[0].2);
                moved.update_value(|m| *m += dx.abs() + dy.abs());
                if moved.get_value() > 5.0 {
                    dragging.set(true);
                    view.update(|v| v.pan(dx, dy, cw, ch));
                }
            }
            2 => {
                let o = old[1 - i];
                let d_old = (old[i].1 - o.1).hypot(old[i].2 - o.2).max(1.0);
                let d_new = (x - o.1).hypot(y - o.2).max(1.0);
                moved.update_value(|m| *m += 100.0);
                let (left, top) = rect_of(ev.current_target())
                    .map(|r| (r.0, r.1))
                    .unwrap_or((0.0, 0.0));
                let (mx, my) = ((x + o.1) / 2.0 - left, (y + o.2) / 2.0 - top);
                view.update(|v| v.zoom_at(d_old / d_new, mx, my, cw, ch, &base));
            }
            _ => {}
        }
        pts.update_value(|p| p[i] = (id, x, y));
    };
    let on_up = move |ev: web_sys::PointerEvent| {
        pts.update_value(|p| p.retain(|q| q.0 != ev.pointer_id()));
        if pts.with_value(|p| p.is_empty()) {
            dragging.set(false);
        }
    };
    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        if let Some((l, t, w, h)) = rect_of(ev.current_target()) {
            let f = (ev.delta_y() / 300.0).exp();
            let (px, py) = (ev.client_x() as f64 - l, ev.client_y() as f64 - t);
            view.update(|v| v.zoom_at(f, px, py, w, h, &base));
        }
    };
    let zoom_btn = move |f: f64| {
        let (w, h) = size.get_value();
        view.update(|v| v.zoom_at(f, w / 2.0, h / 2.0, w, h, &base));
    };

    let mut sea_idx = 0;
    let locs = geometry
        .locations
        .iter()
        .map(|l| {
            let alt = l.kind == LocKind::Sea && {
                sea_idx += 1;
                sea_idx % 2 == 0
            };
            let (id, kind) = (l.id.clone(), l.kind);
            let (id_c, id_h, id_s) = (id.clone(), id.clone(), id.clone());
            view! {
                <path
                    d=l.path.clone()
                    class=move || model.with(|m| loc_class(kind, alt, &id_c, m))
                    on:pointerenter=move |_| hover.set(Some(id_h.clone()))
                    on:pointerleave=move |_| hover.set(None)
                    on:click=move |_| {
                        if moved.get_value() < 6.0 {
                            on_select.run(Some(id_s.clone()));
                        }
                    }
                />
            }
        })
        .collect_view();

    let overlays = geometry
        .locations
        .iter()
        .map(|l| {
            let (lx, ly) = l.label.unwrap_or_default();
            let (ax, ay) = l.anchor.unwrap_or_default();
            let sea = l.kind == LocKind::Sea;
            let id = l.id.clone();
            let ipc = l.ipc.filter(|_| !sea);
            view! {
                <text class=if sea { "lbl sea" } else { "lbl" } x=lx y=ly>{l.name.clone()}</text>
                {ipc.map(|n| view! {
                    <g>
                        <circle class="ipc-bg" cx=lx cy=ly + 11.0 r="7" />
                        <text class="ipc" x=lx y=ly + 14.0>{n}</text>
                    </g>
                })}
                {move || {
                    let stacks = model.with(|m| m.stacks.get(&id).cloned().unwrap_or_default());
                    let n = stacks.len().min(6) as f64;
                    stacks
                        .into_iter()
                        .take(6)
                        .enumerate()
                        .map(|(i, s)| view! {
                            <foreignObject
                                x=ax - 24.0
                                y=ay - n * 10.5 + i as f64 * 21.0
                                width="48"
                                height="20"
                                style="pointer-events:none;overflow:visible"
                            >
                                <UnitBadge kind=s.kind power=s.power count=s.count />
                            </foreignObject>
                        })
                        .collect_view()
                }}
            }
        })
        .collect_view();

    let names: HashMap<String, (String, Option<u32>)> = geometry
        .locations
        .iter()
        .map(|l| (l.id.clone(), (l.name.clone(), l.ipc)))
        .collect();

    view! {
        <div
            class="board-wrap"
            class:dragging=move || dragging.get()
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=on_up
            on:pointerleave=on_up
            on:wheel=on_wheel
        >
            <svg class="board-svg" viewBox=move || view.get().attr()>
                {locs}
                {overlays}
            </svg>
            <div class="board-ctl">
                <button class="secondary small" on:click=move |_| zoom_btn(0.8)>"+"</button>
                <button class="secondary small" on:click=move |_| zoom_btn(1.25)>"\u{2212}"</button>
                <button class="secondary small" on:click=move |_| view.set(base)>"Reset"</button>
                <span class="pill">
                    {move || {
                        hover
                            .get()
                            .and_then(|id| names.get(&id).cloned().map(|(n, i)| (id, n, i)))
                            .map(|(id, n, ipc)| {
                                let owner = model
                                    .with(|m| m.owners.get(&id).map(|p| p.name()))
                                    .unwrap_or("");
                                format!("{n} {owner} {}", ipc.map(|i| format!("({i} IPC)")).unwrap_or_default())
                            })
                            .unwrap_or_else(|| "Hover a territory".into())
                    }}
                </span>
            </div>
        </div>
    }
}
