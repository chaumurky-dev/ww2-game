//! Evaluation helpers: a tiny battle simulator, territory values, the threat map and the front
//! gradient used to steer reinforcements.
use crate::Style;
use std::collections::VecDeque;
use wartide_engine::tech::{attack_value, defense_value};
use wartide_engine::{GameState, LocationId, MapData, Power, Unit, UnitType};

/// Display name of a location.
pub fn name(l: LocationId) -> String {
    MapData::get().loc(l).name.clone()
}

/// Short name of a power for chat.
pub fn power_short(p: Power) -> &'static str {
    match p {
        Power::Ussr => "USSR",
        Power::Germany => "Germany",
        Power::Uk => "UK",
        Power::Japan => "Japan",
        Power::Usa => "USA",
    }
}

/// Cheap deterministic RNG (xorshift) for the mini simulator and tie-breaks.
#[derive(Clone)]
pub struct Xs(pub u64);

impl Xs {
    /// Seeded generator (never zero).
    pub fn new(seed: u64) -> Xs {
        let mut x = Xs(seed ^ 0x9E37_79B9_7F4A_7C15);
        for _ in 0..3 {
            x.next_u64();
        }
        x
    }
    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        if self.0 == 0 {
            self.0 = 0x1234_5678_9ABC_DEF1;
        }
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        if n <= 1 {
            0
        } else {
            (self.next_u64() >> 11) as usize % n
        }
    }
    /// Die roll 1..=6.
    pub fn d6(&mut self) -> u8 {
        (self.below(6) + 1) as u8
    }
}

/// Mix several numbers into a seed.
pub fn mix(parts: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in parts {
        h ^= p;
        h = h.wrapping_mul(0x0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

/// A unit as the mini simulator sees it.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    /// Hit number when attacking.
    pub atk: u8,
    /// Hit number when defending.
    pub def: u8,
    /// IPC cost.
    pub cost: u32,
    /// Counts as a land unit able to take a territory.
    pub land: bool,
    /// Aircraft.
    pub air: bool,
}

/// Piece for an attacker.
pub fn piece(state: &GameState, u: &Unit) -> Piece {
    Piece {
        atk: attack_value(state, u),
        def: defense_value(state, u),
        cost: u.kind.stats().cost,
        land: matches!(u.kind, UnitType::Infantry | UnitType::Armor),
        air: u.kind.is_air(),
    }
}

/// Result of a simulation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Odds {
    /// Attacker win probability.
    pub win: f64,
    /// Expected IPC value the attacker loses.
    pub lose: f64,
    /// Expected IPC value the defender loses.
    pub kill: f64,
}

/// Monte-Carlo estimate of a battle between lists of pieces (cheapest die first).
pub fn sim(
    att: &[Piece],
    def: &[Piece],
    aa: bool,
    need_land: bool,
    trials: u32,
    seed: u64,
) -> Odds {
    let mut rng = Xs::new(seed);
    let (mut wins, mut lose, mut kill) = (0u32, 0f64, 0f64);
    let a0: u32 = att.iter().map(|p| p.cost).sum();
    let d0: u32 = def.iter().map(|p| p.cost).sum();
    let mut sa = att.to_vec();
    let mut sd = def.to_vec();
    sa.sort_by_key(|p| (p.cost, p.atk));
    sd.sort_by_key(|p| (p.cost, p.def));
    for _ in 0..trials.max(1) {
        let mut a = sa.clone();
        let mut d = sd.clone();
        if aa {
            a.retain(|p| !p.air || rng.d6() != 1);
        }
        for _ in 0..30 {
            if a.is_empty() || d.is_empty() {
                break;
            }
            let ah = a.iter().filter(|p| p.atk > 0 && rng.d6() <= p.atk).count();
            let dh = d.iter().filter(|p| p.def > 0 && rng.d6() <= p.def).count();
            d.drain(..ah.min(d.len()));
            // attackers keep a land unit alive if possible
            let mut k = dh.min(a.len());
            let mut i = 0;
            while k > 0 && i < a.len() {
                let last_land = need_land && a[i].land && a.iter().filter(|p| p.land).count() == 1;
                if last_land && a.len() > 1 {
                    i += 1;
                    continue;
                }
                a.remove(i);
                k -= 1;
            }
        }
        let ok = d.is_empty() && !a.is_empty() && (!need_land || a.iter().any(|p| p.land));
        if ok {
            wins += 1;
        }
        lose += f64::from(a0 - a.iter().map(|p| p.cost).sum::<u32>());
        kill += f64::from(d0 - d.iter().map(|p| p.cost).sum::<u32>());
    }
    let t = f64::from(trials.max(1));
    Odds {
        win: f64::from(wins) / t,
        lose: lose / t,
        kill: kill / t,
    }
}

/// Territory worth to `me` if taken (income, capital, complex).
pub fn value(state: &GameState, me: Power, l: LocationId) -> f64 {
    let map = MapData::get();
    if map.is_sea(l) {
        return 0.0;
    }
    let mut v = f64::from(map.ipc(l));
    if let Some(c) = map.capital_of(l) {
        v += if c.is_ally(me) { 14.0 } else { 12.0 };
    }
    if state.complex_at(l).is_some() {
        v += 6.0;
    }
    if map.original_owner(l).is_some_and(|o| o.is_ally(me)) {
        v += 2.0;
    }
    v
}

/// BFS over the location graph from `src`. `land_only` restricts to non-neutral land.
pub fn bfs(
    state: &GameState,
    src: LocationId,
    max: u8,
    land_only: bool,
    sea_only: bool,
) -> Vec<(LocationId, u8)> {
    let map = MapData::get();
    let mut seen = vec![false; map.len()];
    let mut out = vec![];
    let mut q = VecDeque::new();
    seen[src.idx()] = true;
    q.push_back((src, 0u8));
    while let Some((c, d)) = q.pop_front() {
        out.push((c, d));
        if d >= max {
            continue;
        }
        for &n in map.neighbours(c) {
            if seen[n.idx()] {
                continue;
            }
            let sea = map.is_sea(n);
            if (land_only && (sea || state.is_untouched_neutral(n))) || (sea_only && !sea) {
                continue;
            }
            seen[n.idx()] = true;
            q.push_back((n, d + 1));
        }
    }
    out
}

/// Strategic picture of the board for one power.
pub struct Intel {
    /// Acting power.
    pub me: Power,
    /// Enemy attack points that can reach a location next round.
    pub threat: Vec<f64>,
    /// Friendly defence points sitting at a location.
    pub defense: Vec<f64>,
    /// Gradient towards valuable enemy territory (lower is closer).
    pub grad: Vec<f64>,
    /// Same for ships: sea distance to a zone touching a valuable enemy territory.
    pub sea_grad: Vec<f64>,
    /// Same for aircraft: graph distance to a valuable enemy territory.
    pub air_grad: Vec<f64>,
}

const FAR: f64 = 1000.0;

impl Intel {
    /// Build the picture from the live state.
    pub fn new(state: &GameState, me: Power) -> Intel {
        let map = MapData::get();
        let n = map.len();
        let mut threat = vec![0.0; n];
        let mut defense = vec![0.0; n];
        for u in &state.units {
            if u.loaded_on.is_some() {
                continue;
            }
            let k = u.kind;
            if u.owner.is_ally(me) {
                if k.is_fighting_land_unit() || k.is_ship() {
                    defense[u.location.idx()] += f64::from(defense_value(state, u));
                }
                continue;
            }
            let atk = f64::from(attack_value(state, u));
            if atk == 0.0 || k == UnitType::AaGun || k == UnitType::IndustrialComplex {
                continue;
            }
            let reach: Vec<(LocationId, u8)> = if k.is_air() {
                bfs(state, u.location, u.kind.stats().movement / 2, false, false)
            } else if k.is_ship() {
                bfs(state, u.location, u.kind.stats().movement, false, true)
            } else {
                bfs(state, u.location, u.kind.stats().movement, true, false)
            };
            for (l, d) in reach {
                threat[l.idx()] += atk * if d <= 1 { 1.0 } else { 0.6 };
            }
        }
        let mut grad = vec![FAR; n];
        let mut sea_grad = vec![FAR; n];
        let mut air_grad = vec![FAR; n];
        for t in map.territories() {
            if !state.is_enemy_controlled(me, t) {
                continue;
            }
            let w = value(state, me, t);
            if w <= 0.0 {
                continue;
            }
            for (l, d) in bfs(state, t, 12, true, false) {
                let g = f64::from(d) * 2.0 - w * 0.15;
                if g < grad[l.idx()] {
                    grad[l.idx()] = g;
                }
            }
            for (l, d) in bfs(state, t, 8, false, false) {
                let g = f64::from(d) - w * 0.1;
                if g < air_grad[l.idx()] {
                    air_grad[l.idx()] = g;
                }
            }
            for &z in map.neighbours(t).iter().filter(|&&z| map.is_sea(z)) {
                for (l, d) in bfs(state, z, 12, false, true) {
                    let g = f64::from(d) * 2.0 - w * 0.15;
                    if g < sea_grad[l.idx()] {
                        sea_grad[l.idx()] = g;
                    }
                }
            }
        }
        Intel {
            me,
            threat,
            defense,
            grad,
            sea_grad,
            air_grad,
        }
    }

    /// Defence points wanted at `l` (the garrison), scaled by style.
    pub fn garrison_need(&self, state: &GameState, style: Style, l: LocationId) -> f64 {
        let t = self.threat[l.idx()];
        if t <= 0.0 {
            return 0.0;
        }
        let f = match style {
            Style::Aggressive => 0.2,
            Style::Balanced => 0.3,
            Style::Cautious => 0.5,
        };
        let m = MapData::get();
        let worth = f64::from(m.ipc(l))
            + if state.complex_at(l).is_some() {
                3.0
            } else {
                0.0
            }
            + if m.capital_of(l).is_some() { 6.0 } else { 0.0 };
        let mut need = t * f * (worth / 4.0).min(1.0);
        if MapData::get().capital_of(l) == Some(self.me) {
            need = need.max(4.0) * 1.3;
        } else if state.complex_at(l).is_some() {
            need = need.max(2.0);
        }
        need
    }
}

/// Win threshold by style.
pub fn threshold(style: Style) -> f64 {
    match style {
        Style::Aggressive => 0.6,
        Style::Balanced => 0.7,
        Style::Cautious => 0.8,
    }
}

/// Loss order key for casualties: lower is lost first.
pub fn loss_key(state: &GameState, u: &Unit) -> i32 {
    let mut k = u.kind.stats().cost as i32 * 10;
    match u.kind {
        UnitType::Transport => {
            if state.cargo_of(u.id).count() > 0 {
                k += 400;
            } else {
                k -= 5;
            }
        }
        UnitType::Carrier => k += 20,
        _ => {}
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inf() -> Piece {
        Piece {
            atk: 1,
            def: 2,
            cost: 3,
            land: true,
            air: false,
        }
    }
    fn arm() -> Piece {
        Piece {
            atk: 3,
            def: 2,
            cost: 5,
            land: true,
            air: false,
        }
    }

    #[test]
    fn sim_is_sane() {
        let big = sim(&[arm(); 6], &[inf(); 2], false, true, 300, 1);
        assert!(big.win > 0.95, "{}", big.win);
        let small = sim(&[inf(); 1], &[inf(); 5], false, true, 300, 1);
        assert!(small.win < 0.05);
        let even = sim(&[arm(); 3], &[inf(); 3], false, true, 500, 7);
        assert!(even.win > 0.3 && even.win < 0.95);
    }

    #[test]
    fn rng_deterministic() {
        let mut a = Xs::new(5);
        let mut b = Xs::new(5);
        assert_eq!(a.next_u64(), b.next_u64());
        assert!(mix(&[1, 2]) != mix(&[2, 1]));
    }
}
