//! Team-chat lines: templates with persona flavour and a per-turn verbosity budget.
use crate::combat::{AttackKind, AttackNote};
use crate::economy::BuyNote;
use crate::eval::{mix, name, power_short};
use crate::{Commentary, Persona, Style, Verbosity};
use wartide_engine::{GameState, LocationId, MapData, Power, UnitType};

/// Which kind of line, for budgeting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Planned statement (purchase, attack, hold).
    Plan,
    /// Reaction to something that just happened.
    Reaction,
}

/// Decides what may be said and keeps the per-turn budget.
#[derive(Default)]
pub struct Chatter {
    turn: u32,
    plan_used: u32,
    react_used: u32,
}

fn fill(t: &str, kv: &[(&str, String)]) -> String {
    let mut s = t.to_string();
    for (k, v) in kv {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "3 infantry, 2 armor and a fighter".
pub fn describe(items: &[(UnitType, u32)]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|&(k, n)| match k {
            UnitType::Infantry => format!("{n} infantry"),
            UnitType::Armor => format!("{n} armor"),
            UnitType::Fighter => plural(n, "fighter", "fighters"),
            UnitType::Bomber => plural(n, "bomber", "bombers"),
            UnitType::Transport => plural(n, "transport", "transports"),
            UnitType::Carrier => plural(n, "carrier", "carriers"),
            UnitType::Battleship => plural(n, "battleship", "battleships"),
            UnitType::Submarine => plural(n, "submarine", "submarines"),
            UnitType::AaGun => plural(n, "AA gun", "AA guns"),
            UnitType::IndustrialComplex => plural(n, "industrial complex", "industrial complexes"),
        })
        .collect();
    match parts.len() {
        0 => "nothing".into(),
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

fn with_article(w: &str) -> String {
    let an = w.starts_with(['a', 'e', 'i', 'o', 'u', 'A']);
    format!("{} {w}", if an { "an" } else { "a" })
}

fn kind_word(k: UnitType) -> &'static str {
    match k {
        UnitType::Infantry => "infantry",
        UnitType::Armor => "armor",
        UnitType::Fighter => "fighter",
        UnitType::Bomber => "bomber",
        UnitType::Battleship => "battleship",
        UnitType::Carrier => "carrier",
        UnitType::Transport => "transport",
        UnitType::Submarine => "submarine",
        UnitType::AaGun => "AA gun",
        UnitType::IndustrialComplex => "factory",
    }
}

/// Line writer bound to a persona.
pub struct Voice<'a> {
    /// Persona.
    pub persona: &'a Persona,
    /// Our power.
    pub me: Power,
    /// Seed for phrase variety.
    pub seed: u64,
}

impl Voice<'_> {
    fn pick<'t>(&self, state: &GameState, tag: u64, opts: &[&'t str]) -> &'t str {
        let h = mix(&[self.seed, u64::from(state.turn.number), tag]);
        opts[(h % opts.len() as u64) as usize]
    }

    fn style3<'t>(
        &self,
        state: &GameState,
        tag: u64,
        a: &[&'t str],
        b: &[&'t str],
        c: &[&'t str],
    ) -> &'t str {
        match self.persona.style {
            Style::Aggressive => self.pick(state, tag, a),
            Style::Balanced => self.pick(state, tag, b),
            Style::Cautious => self.pick(state, tag, c),
        }
    }

    fn ally(&self, state: &GameState, tag: u64) -> String {
        let allies: Vec<Power> = self
            .me
            .team()
            .powers()
            .iter()
            .copied()
            .filter(|&p| p != self.me)
            .collect();
        if allies.is_empty() {
            return "team".into();
        }
        let h = mix(&[self.seed, u64::from(state.turn.number), tag]);
        power_short(allies[(h % allies.len() as u64) as usize]).into()
    }

    /// The enemy territory worth the most near our capital (for "next round I'll ferry to ...").
    fn focus(&self, state: &GameState) -> Option<LocationId> {
        let map = MapData::get();
        let near = crate::eval::bfs(state, self.me.capital(), 9, false, false);
        near.into_iter()
            .filter(|&(l, _)| !map.is_sea(l) && state.is_enemy_controlled(self.me, l))
            .max_by(|a, b| {
                let sa = crate::eval::value(state, self.me, a.0) / (1.0 + f64::from(a.1) / 3.0);
                let sb = crate::eval::value(state, self.me, b.0) / (1.0 + f64::from(b.1) / 3.0);
                sa.total_cmp(&sb).then(b.0.cmp(&a.0))
            })
            .map(|x| x.0)
    }

    /// Purchase announcement.
    pub fn purchase(&self, state: &GameState, note: &BuyNote) -> Vec<String> {
        if note.items.is_empty() {
            return vec![];
        }
        let items = describe(&note.items);
        let kv = [("items", items), ("spent", note.spent.to_string())];
        let mut out = vec![];
        let base = self.style3(
            state,
            1,
            &[
                "Buying {items} ({spent} IPC) - I want the next wave ready to roll.",
                "Shopping list: {items}. {spent} IPC well spent, let's go on the offensive.",
            ],
            &[
                "Purchase this turn: {items} for {spent} IPC.",
                "Buying {items} ({spent} IPC) to keep our production working.",
            ],
            &[
                "Picking up {items} ({spent} IPC). Better solid than sorry.",
                "I'll buy {items} for {spent} IPC and keep a steady hand.",
            ],
        );
        out.push(fill(base, &kv));
        let transports = note
            .items
            .iter()
            .find(|i| i.0 == UnitType::Transport)
            .map_or(0, |i| i.1);
        if transports > 0 {
            let dest = self.focus(state).map_or("the front".to_string(), name);
            let n = plural(transports, "transport", "transports");
            out.push(format!(
                "Buying {n} - I'll start ferrying toward {dest} next round."
            ));
        }
        if let Some(site) = note.complex_at {
            out.push(format!(
                "New industrial complex going into {} - more production closer to the action.",
                name(site)
            ));
        }
        if note.defensive {
            out.push(format!(
                "{} looks exposed, so this one is mostly about defence.",
                name(self.me.capital())
            ));
        }
        out
    }

    /// Research announcement.
    pub fn research(&self, state: &GameState) -> Vec<String> {
        vec![self
            .pick(
                state,
                2,
                &[
                    "Rolling a research die - a lucky breakthrough could swing things.",
                    "Spending 5 IPC on research. Fingers crossed.",
                ],
            )
            .to_string()]
    }

    fn units_phrase(n: &AttackNote) -> String {
        let mut parts = vec![];
        if n.inf > 0 {
            parts.push(format!("{} infantry", n.inf));
        }
        if n.arm > 0 {
            parts.push(format!("{} armor", n.arm));
        }
        if n.air > 0 {
            parts.push(plural(n.air, "aircraft", "aircraft"));
        }
        if n.ships > 0 {
            parts.push(plural(n.ships, "ship", "ships"));
        }
        match parts.len() {
            0 => "a few units".into(),
            1 => parts[0].clone(),
            k => format!("{} and {}", parts[..k - 1].join(", "), parts[k - 1]),
        }
    }

    /// Summary of the combat-move plan, followed by a coordination request.
    pub fn attacks(&self, state: &GameState, notes: &[AttackNote]) -> Vec<String> {
        if notes.is_empty() {
            return vec![self
                .style3(
                    state,
                    3,
                    &["Nothing worth hitting this turn - I'll build up and strike next round."],
                    &["No attacks this turn; nothing at acceptable odds."],
                    &["Not seeing any safe attacks, so I'm staying put and digging in."],
                )
                .to_string()];
        }
        let best = notes
            .iter()
            .filter(|n| n.kind != AttackKind::Grab)
            .max_by(|a, b| a.win.total_cmp(&b.win))
            .or_else(|| notes.first())
            .unwrap();
        let t = name(best.target);
        let pct = (best.win * 100.0).round() as u32;
        let up = Self::units_phrase(best);
        let kv = [
            ("up", up),
            ("t", t.clone()),
            ("pct", pct.to_string()),
            ("n", notes.len().to_string()),
        ];
        let line = match best.kind {
            AttackKind::Land => self.style3(
                state,
                4,
                &[
                    "Pushing {up} into {t} this turn (odds ~{pct}%). No more waiting!",
                    "Hitting {t} with {up} (odds ~{pct}%). Let's take it.",
                ],
                &[
                    "Attacking {t} with {up}, odds ~{pct}%.",
                    "Moving {up} against {t} this turn (odds ~{pct}%).",
                ],
                &[
                    "Carefully committing {up} against {t} (odds ~{pct}%) - only because the numbers look good.",
                    "Taking a measured shot at {t} with {up}, odds ~{pct}%.",
                ],
            ),
            AttackKind::Amphibious => self.style3(
                state,
                5,
                &["Landing {up} at {t} from the transports (odds ~{pct}%) - they won't see it coming!"],
                &["Amphibious landing at {t} with {up}, odds ~{pct}%."],
                &["Putting {up} ashore at {t} (odds ~{pct}%). The transports are covered, I hope."],
            ),
            AttackKind::Sea => self.style3(
                state,
                6,
                &["Going after the fleet at {t} with {up} (odds ~{pct}%). Sink them!"],
                &["Naval strike at {t} with {up}, odds ~{pct}%."],
                &["The fleet at {t} is exposed - striking with {up} at ~{pct}%."],
            ),
            AttackKind::Bombing => self.style3(
                state,
                7,
                &["Sending {up} to flatten the factory at {t}."],
                &["Strategic bombing run on {t} with {up}."],
                &["One bombing raid on {t} - a cheap way to hurt their production."],
            ),
            AttackKind::Grab => self.style3(
                state,
                8,
                &["Walking into undefended {t} - free IPCs."],
                &["Occupying undefended {t}."],
                &["Quietly taking undefended {t}."],
            ),
        };
        let mut out = vec![fill(line, &kv)];
        if notes.len() > 1 {
            out[0].push_str(&fill(" That's {n} operations in all.", &kv));
        }
        let ally = self.ally(state, 9);
        let ask = match best.kind {
            AttackKind::Amphibious => self.pick(
                state,
                10,
                &[
                    "{ally}, can you keep the sea lanes near {t} covered?",
                    "If you can, {ally}, keep an escort near {t} so my transports survive.",
                ],
            ),
            AttackKind::Sea => self.pick(
                state,
                10,
                &[
                    "{ally}, if you can bring air cover over {t}, the fleet fight gets much easier.",
                    "Keep an eye on {t}, {ally} - I'd hate to leave the sea lanes open afterwards.",
                ],
            ),
            _ => self.pick(
                state,
                11,
                &[
                    "{ally}, can you apply pressure elsewhere so they can't shift everything to {t}?",
                    "If you can, {ally}, hit them near {t} too - splitting their defence helps.",
                    "{ally}: please keep your home garrison topped up while I lean forward.",
                ],
            ),
        };
        out.push(fill(ask, &[("ally", ally), ("t", t)]));
        out
    }

    /// After combat: what we captured.
    pub fn results(&self, state: &GameState, attempted: bool) -> Vec<String> {
        let map = MapData::get();
        let taken: Vec<String> = map
            .territories()
            .filter(|&t| {
                state.control[t] == Some(self.me) && state.turn.start_control[t] != Some(self.me)
            })
            .map(name)
            .collect();
        if !taken.is_empty() {
            let inc = wartide_engine::income(state, self.me);
            let l = self.pick(
                state,
                12,
                &[
                    "Took {list}! Income should be around {inc} IPC now.",
                    "{list} is ours - that puts my income near {inc} IPC.",
                ],
            );
            return vec![fill(
                l,
                &[("list", taken.join(" and ")), ("inc", inc.to_string())],
            )];
        }
        if attempted {
            return vec![self
                .style3(
                    state,
                    13,
                    &["That didn't go to plan, but we'll hit again next round."],
                    &["The attacks didn't break through. Regrouping."],
                    &["Not the result I wanted. I'll regroup before trying anything else."],
                )
                .to_string()];
        }
        vec![]
    }

    /// After non-combat: where we are holding.
    pub fn hold(&self, state: &GameState, at: Option<LocationId>, moved: u32) -> Vec<String> {
        let Some(at) = at.filter(|_| moved > 0) else {
            return vec![];
        };
        let l = self.style3(
            state,
            14,
            &[
                "Shifting {n} up behind {t} - ready for the next push.",
                "Holding the line at {t} and massing {n} for the next attack.",
            ],
            &[
                "Holding the line at {t}; {n} repositioned.",
                "{n} moved up to cover {t}.",
            ],
            &[
                "Digging in around {t}; {n} moved to cover it.",
                "Holding {t} firmly. {n} shuffled to support it.",
            ],
        );
        vec![fill(
            l,
            &[("t", name(at)), ("n", plural(moved, "unit", "units"))],
        )]
    }

    /// Reaction to losses taken at `at`.
    pub fn loss(&self, state: &GameState, kinds: &[UnitType], at: LocationId) -> Option<String> {
        let top = kinds
            .iter()
            .copied()
            .max_by_key(|k| k.stats().cost)
            .filter(|k| k.stats().cost >= 5)?;
        let home = name(self.me.capital());
        let l = self.style3(
            state,
            15,
            &["Lost {ka} over {t}, ouch. Next time they pay double."],
            &["Lost the {k} at {t}. Holding the line at {home}."],
            &["Ouch, the {k} at {t} is gone. Time to be more careful around {home}."],
        );
        Some(fill(
            l,
            &[
                ("k", kind_word(top).into()),
                ("t", name(at)),
                ("home", home),
            ],
        ))
    }

    /// Reaction to a retreat.
    pub fn retreat(&self, state: &GameState, at: LocationId) -> String {
        let l = self.pick(
            state,
            16,
            &[
                "Pulling back from {t} - the odds went bad.",
                "That fight at {t} turned against us, so I'm withdrawing.",
            ],
        );
        fill(l, &[("t", name(at))])
    }
}

impl Chatter {
    fn reset(&mut self, state: &GameState) {
        if self.turn != state.turn.number {
            *self = Chatter {
                turn: state.turn.number,
                plan_used: 0,
                react_used: 0,
            };
        }
    }

    /// Apply the verbosity budget to candidate lines.
    pub fn filter(
        &mut self,
        state: &GameState,
        persona: &Persona,
        seed: u64,
        kind: Kind,
        lines: Vec<String>,
    ) -> Vec<Commentary> {
        self.reset(state);
        let (plan_cap, react_cap) = match persona.verbosity {
            Verbosity::Quiet => {
                if (u64::from(state.round) + seed).is_multiple_of(2) {
                    (1, 0)
                } else {
                    (0, 0)
                }
            }
            Verbosity::Normal => (2, 0),
            Verbosity::Chatty => (4, 3),
        };
        let mut out = vec![];
        for l in lines {
            let (used, cap) = match kind {
                Kind::Plan => (&mut self.plan_used, plan_cap),
                Kind::Reaction => (&mut self.react_used, react_cap),
            };
            if *used >= cap {
                break;
            }
            *used += 1;
            out.push(Commentary { text: l });
        }
        out
    }
}
