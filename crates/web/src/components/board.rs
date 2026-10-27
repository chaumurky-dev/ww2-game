//! SVG game board with pan/zoom. Decoupled from the engine through [`BoardModel`].
use super::unit_icon::{UnitBadge, UnitIcon};
use super::viewport::ViewBox;
use crate::map::{sea_shade, LocKind, SharedMap};
use crate::model::{Power, UnitKind};
use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

/// A stack of identical units at a location.
#[derive(Debug, Clone, PartialEq)]
pub struct Stack {
    pub power: Power,
    pub kind: UnitKind,
    pub count: u32,
    /// Units carried by this stack's ships (transports / carriers), by kind.
    pub cargo: Vec<(UnitKind, u32)>,
    /// How many units of this stack are currently picked for a move.
    pub picked: u32,
    /// The viewer may drag this stack's counter to move its units.
    pub draggable: bool,
}

/// Result of a unit drag, handed to the game screen when the pointer is released.
#[derive(Debug, Clone, PartialEq)]
pub struct DragEnd {
    pub from: String,
    pub kind: UnitKind,
    /// Territory under the pointer on release (`None` when cancelled).
    pub dest: Option<String>,
    /// Pointer position in client pixels.
    pub x: f64,
    pub y: f64,
}

/// A unit-stack drag in progress (or a pointerdown on a handle that may still turn into a tap).
#[derive(Debug, Clone, PartialEq)]
struct DragSt {
    from: String,
    kind: UnitKind,
    power: Power,
    count: u32,
    pointer: i32,
    start: (f64, f64),
    active: bool,
    /// Pointer position in SVG user units.
    at: (f64, f64),
}

/// Pointer travel (CSS px) before a press on a counter becomes a drag instead of a tap.
const DRAG_THRESHOLD: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    /// Legal destination.
    Move,
    /// Legal destination that starts a battle.
    Attack,
    /// Battle site.
    Battle,
    /// Legal placement.
    Place,
    /// Holds units that can still act.
    Movable,
}

/// View-model for the board; to be filled from the engine's `GameState`.
/// Keys are geometry location ids (same ids as `map.json`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardModel {
    pub owners: HashMap<String, Power>,
    pub stacks: HashMap<String, Vec<Stack>>,
    pub highlights: HashMap<String, Highlight>,
    pub selected: Option<String>,
    /// Locations touched by the latest events (brief highlight animation).
    pub flash: std::collections::HashSet<String>,
    /// Hover text (multi-line) per location.
    pub tips: HashMap<String, String>,
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
        Some(Highlight::Place) => c.push_str(" hl-place"),
        Some(Highlight::Movable) => c.push_str(" hl-movable"),
        None => {}
    }
    if m.selected.as_deref() == Some(id) {
        c.push_str(" selected");
    }
    if m.flash.contains(id) {
        c.push_str(" flash");
    }
    c
}

fn rect_of(target: Option<web_sys::EventTarget>) -> Option<(f64, f64, f64, f64)> {
    let el = target?.dyn_into::<web_sys::Element>().ok()?;
    let r = el.get_bounding_client_rect();
    Some((r.left(), r.top(), r.width(), r.height()))
}

/// The territory (`path[data-id]`) under a client point, ignoring overlays.
fn territory_at(x: f64, y: f64) -> Option<String> {
    let doc = web_sys::window()?.document()?;
    doc.elements_from_point(x as f32, y as f32)
        .iter()
        .filter_map(|v| v.dyn_into::<web_sys::Element>().ok())
        .find_map(|e| e.get_attribute("data-id"))
}

type Pointers = Vec<(i32, f64, f64)>;

#[component]
pub fn Board(
    map: SharedMap,
    #[prop(into)] model: Signal<BoardModel>,
    on_select: Callback<Option<String>>,
    /// When it changes to `Some(id)` the view centres and zooms on that location.
    #[prop(optional, into)]
    focus: Option<Signal<Option<String>>>,
    /// A unit counter was picked up (after the pointer travelled past the drag threshold).
    #[prop(optional)]
    on_drag_start: Option<Callback<(String, UnitKind)>>,
    /// A unit drag ended (dropped on a territory or cancelled).
    #[prop(optional)]
    on_drag_end: Option<Callback<DragEnd>>,
) -> impl IntoView {
    let base = ViewBox::new(map.geo.view_box);
    let view = RwSignal::new(base);
    let hover = RwSignal::new(None::<String>);
    let dragging = RwSignal::new(false);
    let pts = StoredValue::new(Pointers::new());
    let moved = StoredValue::new(0.0_f64);
    let size = StoredValue::new((800.0_f64, 600.0_f64));
    let wrap = NodeRef::<leptos::html::Div>::new();
    let drag = RwSignal::new(None::<DragSt>);
    let drag_active = move || drag.with(|d| d.as_ref().is_some_and(|d| d.active));
    if let Some(f) = focus {
        let anchors: HashMap<String, (f64, f64)> = map
            .geo
            .locations
            .iter()
            .map(|l| (l.id.clone(), l.anchor))
            .collect();
        // The view before the first focus; restored when the battle is over.
        let saved = StoredValue::new(None::<ViewBox>);
        Effect::new(move |_| {
            let target = f.get();
            if target.is_none() {
                if let Some(v) = saved.get_value() {
                    saved.set_value(None);
                    view.set(v);
                }
                return;
            }
            if let Some((x, y)) = target.and_then(|id| anchors.get(&id).copied()) {
                if saved.get_value().is_none() {
                    saved.set_value(Some(view.get_untracked()));
                }
                let (w, h) = (base.w * 0.33, base.h * 0.33);
                view.set(ViewBox {
                    x: x - w / 2.0,
                    y: y - h / 2.0,
                    w,
                    h,
                });
            }
        });
    }

    // Unit drags are tracked on the window so they survive the counter being re-rendered.
    let end_drag = move |dest: Option<String>, x: f64, y: f64| {
        let Some(d) = drag.get_untracked() else {
            return;
        };
        drag.set(None);
        if d.active {
            if let Some(cb) = on_drag_end {
                cb.run(DragEnd {
                    from: d.from,
                    kind: d.kind,
                    dest,
                    x,
                    y,
                });
            }
        } else if dest.is_some() {
            // A tap on the counter behaves like a click on its territory.
            on_select.run(Some(d.from));
        }
    };
    let h_move = window_event_listener(leptos::ev::pointermove, move |ev| {
        let Some(d) = drag.get_untracked() else {
            return;
        };
        if d.pointer != ev.pointer_id() {
            return;
        }
        let (cx, cy) = (ev.client_x() as f64, ev.client_y() as f64);
        let mut next = d.clone();
        if !d.active && (cx - d.start.0).hypot(cy - d.start.1) > DRAG_THRESHOLD {
            next.active = true;
            if let Some(cb) = on_drag_start {
                cb.run((d.from.clone(), d.kind));
            }
        }
        if next.active {
            if let Some((l, t, w, h)) = rect_of(wrap.get_untracked().map(|e| e.into())) {
                next.at = view.get_untracked().to_user(cx - l, cy - t, w, h);
            }
        }
        if next != d {
            drag.set(Some(next));
        }
    });
    let h_up = window_event_listener(leptos::ev::pointerup, move |ev| {
        let Some(d) = drag.get_untracked() else {
            return;
        };
        if d.pointer != ev.pointer_id() {
            return;
        }
        let (cx, cy) = (ev.client_x() as f64, ev.client_y() as f64);
        let dest = if d.active {
            territory_at(cx, cy)
        } else {
            Some(d.from.clone())
        };
        end_drag(dest, cx, cy);
    });
    let h_cancel = window_event_listener(leptos::ev::pointercancel, move |ev| {
        if drag
            .get_untracked()
            .is_some_and(|d| d.pointer == ev.pointer_id())
        {
            end_drag(None, ev.client_x() as f64, ev.client_y() as f64);
        }
    });
    let h_key = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" && drag_active() {
            end_drag(None, 0.0, 0.0);
        }
    });
    on_cleanup(move || {
        h_move.remove();
        h_up.remove();
        h_cancel.remove();
        h_key.remove();
    });

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

    // Zoom level: 0 = whole map, 1 = labels for land, 2 = labels for sea zones too.
    let level = Memo::new(move |_| {
        let z = view.get().w / base.w;
        if z < 0.35 {
            2
        } else if z < 0.65 {
            1
        } else {
            0
        }
    });

    let locs = map
        .geo
        .locations
        .iter()
        .map(|l| {
            let alt = l.kind == LocKind::Sea && sea_shade(&l.id);
            let (id, kind) = (l.id.clone(), l.kind);
            let (id_c, id_h, id_s) = (id.clone(), id.clone(), id.clone());
            view! {
                <path
                    d=l.path.clone()
                    data-id=id.clone()
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

    let overlays = map
        .geo
        .locations
        .iter()
        .map(|l| {
            let sea = l.kind == LocKind::Sea;
            let id = l.id.clone();
            let info = map.data.info.get(&l.id).cloned();
            let (lx, ly) = l.label;
            let (ax, ay) = l.anchor;
            let name = info
                .as_ref()
                .map(|i| if sea { i.short.clone() } else { i.name.clone() })
                .unwrap_or_else(|| l.id.clone());
            let ipc = info.as_ref().map(|i| i.ipc).filter(|n| !sea && *n > 0);
            let min_level = if sea { 2 } else { 1 };
            view! {
                <g class="ov" style=format!("transform:translate({lx}px,{ly}px) scale(var(--zs))")>
                    <text class={if sea { "lbl sea" } else { "lbl" }} y="-4" style:display={move || if level.get() >= min_level { "inline" } else { "none" }}>
                        {name}
                    </text>
                    {ipc.map(|n| view! {
                        <g class="ipc-g">
                            <circle class="ipc-bg" cy="8" r="7" />
                            <text class="ipc" y="11.5">{n}</text>
                        </g>
                    })}
                </g>
                <g class="ov" style=format!("transform:translate({ax}px,{ay}px) scale(var(--zs))")>
                    {move || {
                        let stacks = model.with(|m| m.stacks.get(&id).cloned().unwrap_or_default());
                        let n = stacks.len().min(6) as f64;
                        stacks
                            .into_iter()
                            .take(6)
                            .enumerate()
                            .map(|(i, s)| {
                                let is_picked = s.picked > 0;
                                let (loc_id, kind, power, count) = (id.clone(), s.kind, s.power, s.count);
                                let can_drag = s.draggable;
                                let press = move |ev: web_sys::PointerEvent| {
                                    if !can_drag || (ev.pointer_type() == "mouse" && ev.button() != 0) {
                                        return;
                                    }
                                    // Not a map pan: keep the board-wrap handler from seeing it.
                                    ev.stop_propagation();
                                    if let Some(el) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                                        // Touch pointers are captured implicitly; release so the
                                        // window listeners keep working if this node is re-rendered.
                                        let _ = el.release_pointer_capture(ev.pointer_id());
                                    }
                                    ev.prevent_default();
                                    let p = (ev.client_x() as f64, ev.client_y() as f64);
                                    drag.set(Some(DragSt {
                                        from: loc_id.clone(),
                                        kind,
                                        power,
                                        count,
                                        pointer: ev.pointer_id(),
                                        start: p,
                                        active: false,
                                        at: (0.0, 0.0),
                                    }));
                                };
                                view! {
                                <foreignObject
                                    x="-24"
                                    y=-n * 10.5 + i as f64 * 21.0
                                    width="150"
                                    height="20"
                                    style="pointer-events:none;overflow:visible"
                                >
                                    <span class="stack-row" class:picked=is_picked>
                                        <span class="drag-handle" class:on=can_drag on:pointerdown=press title=can_drag.then_some("Drag onto a destination to move")>
                                            <UnitBadge kind=s.kind power=s.power count=s.count />
                                        </span>
                                        {(!s.cargo.is_empty()).then(|| view! {
                                            <span class="cargo" title="Cargo">
                                                {s.cargo.iter().map(|(k, n)| view! { <span class="cg"><UnitIcon kind=*k />{format!("{n}")}</span> }).collect_view()}
                                            </span>
                                        })}
                                    </span>
                                </foreignObject>
                            }})
                            .collect_view()
                    }}
                </g>
            }
        })
        .collect_view();

    let map_h = map.clone();
    let decor = map.geo.decor.clone();

    view! {
        <div
            class="board-wrap"
            class:dragging=move || dragging.get()
            class:unit-drag=drag_active
            node_ref=wrap
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=on_up
            on:pointerleave=on_up
            on:wheel=on_wheel
        >
            <svg
                class="board-svg"
                viewBox=move || view.get().attr()
                style=move || format!("--zs:{:.3}", (view.get().w / base.w * 1.1).clamp(0.2, 1.0))
            >
                {decor.map(|d| view! { <path class="decor" d=d /> })}
                {locs}
                {overlays}
                {move || drag.get().filter(|d| d.active).map(|d| {
                    let (x, y) = d.at;
                    view! {
                        <g class="ov drag-ghost" style=format!("transform:translate({x}px,{y}px) scale(var(--zs))")>
                            <foreignObject x="-14" y="-10" width="150" height="20" style="pointer-events:none;overflow:visible">
                                <span class="stack-row"><UnitBadge kind=d.kind power=d.power count=d.count /></span>
                            </foreignObject>
                        </g>
                    }
                })}
            </svg>
            <div class="board-ctl">
                <button class="secondary small" on:click=move |_| zoom_btn(0.8)>"+"</button>
                <button class="secondary small" on:click=move |_| zoom_btn(1.25)>"\u{2212}"</button>
                <button class="secondary small" on:click=move |_| view.set(base)>"Reset"</button>
                <span class="pill tip">
                    {move || {
                        hover
                            .get()
                            .map(|id| model.with(|m| m.tips.get(&id).cloned()).unwrap_or_else(|| map_h.data.name(&id).to_string()))
                            .unwrap_or_else(|| "Hover a territory".into())
                    }}
                </span>
            </div>
        </div>
    }
}

/// Static, non-interactive map (lobby / landing preview) coloured by original owners.
#[component]
pub fn MapPreview(map: SharedMap) -> impl IntoView {
    let model = BoardModel {
        owners: map
            .data
            .info
            .iter()
            .filter_map(|(id, i)| i.owner.map(|p| (id.clone(), p)))
            .collect(),
        ..Default::default()
    };
    let vb = ViewBox::new(map.geo.view_box).attr();
    let paths = map
        .geo
        .locations
        .iter()
        .map(|l| {
            let c = loc_class(
                l.kind,
                l.kind == LocKind::Sea && sea_shade(&l.id),
                &l.id,
                &model,
            );
            view! { <path d=l.path.clone() class=c style="pointer-events:none" /> }
        })
        .collect_view();
    let decor = map.geo.decor.clone();
    view! {
        <svg class="board-svg preview" viewBox=vb>
            {decor.map(|d| view! { <path class="decor" d=d /> })}
            {paths}
        </svg>
    }
}
