//! Bots-vs-bots harness used by tests and `examples/selfplay.rs`.
use crate::{Bot, Persona, Style, Verbosity};
use wartide_engine::event::BattleOutcome;
use wartide_engine::{
    apply, income, pending_decision, Decision, Event, GameSettings, GameState, MapData, Power,
};

/// Per-round snapshot.
#[derive(Clone, Debug)]
pub struct RoundSummary {
    /// Round number.
    pub round: u32,
    /// Income per power at the end of the round.
    pub income: [u32; 5],
    /// Territories controlled per power.
    pub territories: [u32; 5],
}

/// Outcome of a self-play game.
pub struct Report {
    /// Final state.
    pub state: GameState,
    /// One entry per completed round.
    pub rounds: Vec<RoundSummary>,
    /// Actions the engine rejected (should stay tiny).
    pub errors: Vec<String>,
    /// Total accepted actions.
    pub actions: u64,
    /// Longest streak of actions within one power-turn.
    pub max_turn_actions: u32,
    /// A sample of chat lines: (round, power, text).
    pub chat: Vec<(u32, Power, String)>,
    /// Worst wall-clock time of one `next_action` call, in milliseconds.
    pub max_call_ms: f64,
    /// Total wall-clock time spent in `next_action`, in milliseconds.
    pub total_ms: f64,
    /// Slowest whole turn (sum of its calls), milliseconds.
    pub max_turn_ms: f64,
    /// Battles per attacking power: [started, won, retreated].
    pub battles: [[u32; 3]; 5],
    /// Units lost to non-combat causes (aircraft that could not land, unplaceable purchases), per power.
    pub lost_noncombat: [u32; 5],
    /// True if the loop aborted because a bot returned nothing or exceeded the action cap.
    pub stalled: bool,
}

fn snapshot(st: &GameState) -> RoundSummary {
    let map = MapData::get();
    let mut inc = [0; 5];
    let mut terr = [0; 5];
    for p in Power::ALL {
        inc[p.index()] = income(st, p);
        terr[p.index()] = map
            .territories()
            .filter(|&t| st.control[t] == Some(p))
            .count() as u32;
    }
    RoundSummary {
        round: st.round,
        income: inc,
        territories: terr,
    }
}

/// Play a full game with the given personas (indexed by `Power::index`).
pub fn play(
    settings: GameSettings,
    styles: [Style; 5],
    verbosity: Verbosity,
    seed: u64,
    max_rounds: u32,
) -> Report {
    play_opts(settings, styles, verbosity, seed, max_rounds, false)
}

fn make_bot(p: Power, style: Style, verbosity: Verbosity, seed: u64) -> Bot {
    Bot::new(
        Persona {
            id: format!("{style:?}").to_lowercase(),
            name: format!("{} bot", p.name()),
            style,
            verbosity,
        },
        p,
        seed ^ (p.index() as u64 + 1).wrapping_mul(0x9E37_79B9),
    )
}

/// Like [`play`]; with `fresh_each_call` a brand-new bot answers every request, which is how a
/// stateless server would drive it.
pub fn play_opts(
    settings: GameSettings,
    styles: [Style; 5],
    verbosity: Verbosity,
    seed: u64,
    max_rounds: u32,
    fresh_each_call: bool,
) -> Report {
    let mut st = GameState::new(settings);
    let mut bots: Vec<Bot> = Power::ALL
        .iter()
        .map(|&p| make_bot(p, styles[p.index()], verbosity, seed))
        .collect();
    let mut rep = Report {
        state: st.clone(),
        rounds: vec![],
        errors: vec![],
        actions: 0,
        max_turn_actions: 0,
        chat: vec![],
        max_call_ms: 0.0,
        total_ms: 0.0,
        max_turn_ms: 0.0,
        battles: [[0; 3]; 5],
        lost_noncombat: [0; 5],
        stalled: false,
    };
    let mut turn_actions = 0u32;
    let mut turn_ms = 0f64;
    let mut turn_no = st.turn.number;
    let mut last_round = st.round;
    let mut seq = 0u64;
    let mut rejected_streak = 0u32;
    while st.winner.is_none() && st.round <= max_rounds {
        let actor = match pending_decision(&st) {
            Decision::GameOver { .. } => break,
            Decision::Phase { power, .. }
            | Decision::SelectCasualties { power, .. }
            | Decision::RetreatOrPress { power, .. }
            | Decision::SubWithdrawal { power, .. } => power,
        };
        if st.turn.number != turn_no {
            rep.max_turn_ms = rep.max_turn_ms.max(turn_ms);
            turn_ms = 0.0;
            turn_actions = 0;
            turn_no = st.turn.number;
        }
        if fresh_each_call {
            bots[actor.index()] = make_bot(actor, styles[actor.index()], verbosity, seed);
        }
        let t0 = std::time::Instant::now();
        let res = bots[actor.index()].next_action(&st);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        rep.total_ms += ms;
        turn_ms += ms;
        rep.max_call_ms = rep.max_call_ms.max(ms);
        let Some((action, chat)) = res else {
            rep.stalled = true;
            rep.errors.push(format!("bot {actor:?} returned None"));
            break;
        };
        seq += 1;
        turn_actions += 1;
        if turn_actions > 1500 {
            rep.stalled = true;
            rep.errors.push(format!("turn cap exceeded by {actor:?}"));
            break;
        }
        match apply(
            &mut st,
            actor,
            &action,
            seed.wrapping_mul(31).wrapping_add(seq),
        ) {
            Ok(events) => {
                for e in &events {
                    match e {
                        Event::UnitsLost { units, .. } => {
                            rep.lost_noncombat[actor.index()] += units.len() as u32
                        }
                        Event::BattleStarted { .. } => rep.battles[actor.index()][0] += 1,
                        Event::BattleEnded { outcome, .. } => match outcome {
                            BattleOutcome::AttackerWins => rep.battles[actor.index()][1] += 1,
                            BattleOutcome::Retreated => rep.battles[actor.index()][2] += 1,
                            _ => {}
                        },
                        _ => {}
                    }
                }
                rejected_streak = 0;
                rep.actions += 1;
                rep.max_turn_actions = rep.max_turn_actions.max(turn_actions);
                for c in chat {
                    rep.chat.push((st.round, actor, c.text));
                }
            }
            Err(e) => {
                rejected_streak += 1;
                if rep.errors.len() < 50 {
                    rep.errors.push(format!(
                        "r{} {:?} {:?}: {:?} -> {e}",
                        st.round, actor, st.phase, action
                    ));
                }
                if rejected_streak > 60 {
                    rep.stalled = true;
                    break;
                }
            }
        }
        if st.round != last_round {
            rep.rounds.push(snapshot(&st));
            last_round = st.round;
        }
    }
    rep.max_turn_ms = rep.max_turn_ms.max(turn_ms);
    rep.state = st;
    rep
}
