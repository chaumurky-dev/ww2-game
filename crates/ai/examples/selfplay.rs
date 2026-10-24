//! Self-play: five bots play a full game; prints a per-round summary and some chat.
use wartide_ai::selfplay::play;
use wartide_ai::{Style, Verbosity};
use wartide_engine::{GameSettings, Power};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("dump") {
        dump(&args);
        return;
    }
    if args.get(1).map(String::as_str) == Some("sweep") {
        sweep(&args);
        return;
    }
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let rounds: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let style = match args.get(3).map(String::as_str) {
        Some("balanced") => Style::Balanced,
        Some("cautious") => Style::Cautious,
        _ => Style::Aggressive,
    };
    let settings = GameSettings {
        seed,
        ..GameSettings::default()
    };
    let rep = play(settings, [style; 5], Verbosity::Chatty, seed, rounds);
    println!("round | income USSR GER UK JPN USA | territories USSR GER UK JPN USA");
    for r in &rep.rounds {
        println!(
            "{:>5} | {:>13} {:>3} {:>2} {:>3} {:>3} | {:>12} {:>3} {:>2} {:>3} {:>3}",
            r.round - 1,
            r.income[0],
            r.income[1],
            r.income[2],
            r.income[3],
            r.income[4],
            r.territories[0],
            r.territories[1],
            r.territories[2],
            r.territories[3],
            r.territories[4],
        );
    }
    match rep.state.winner {
        Some(w) => println!(
            "winner: {:?} ({:?}) in round {}",
            w.team, w.reason, rep.state.round
        ),
        None => println!("no winner after {} rounds", rep.state.round - 1),
    }
    println!(
        "actions {}, rejected {}, max actions/turn {}, slowest call {:.1} ms, slowest turn {:.1} ms, total {:.0} ms",
        rep.actions, rep.errors.len(), rep.max_turn_actions, rep.max_call_ms, rep.max_turn_ms, rep.total_ms
    );
    for p in Power::ALL {
        let b = rep.battles[p.index()];
        println!(
            "{:<14} battles {:>3} won {:>3} retreated {:>3}",
            p.name(),
            b[0],
            b[1],
            b[2]
        );
    }
    for e in rep.errors.iter().take(10) {
        println!("  error: {e}");
    }
    println!("--- chat samples ---");
    let step = (rep.chat.len() / 14).max(1);
    for (round, p, text) in rep.chat.iter().step_by(step).take(14) {
        println!("[r{round} {}] {text}", p.name());
    }
    let _ = Power::Ussr;
}

fn sweep(args: &[String]) {
    let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);
    let style = match args.get(3).map(String::as_str) {
        Some("balanced") => Style::Balanced,
        Some("cautious") => Style::Cautious,
        _ => Style::Aggressive,
    };
    let (mut axis, mut allies, mut none) = (0, 0, 0);
    for seed in 1..=n {
        let rep = play(
            GameSettings {
                seed,
                ..GameSettings::default()
            },
            [style; 5],
            Verbosity::Normal,
            seed,
            30,
        );
        let res = match rep.state.winner {
            Some(w) => format!("{:?} {:?}", w.team, w.reason),
            None => "none".into(),
        };
        match rep.state.winner.map(|w| w.team) {
            Some(wartide_engine::Team::Axis) => axis += 1,
            Some(_) => allies += 1,
            None => none += 1,
        }
        let last = rep.rounds.last();
        println!(
            "seed {seed:>3}: {res:<22} round {:>2}  rejected {} slowest turn {:.0} ms  last income {:?}",
            rep.state.round,
            rep.errors.len(),
            rep.max_turn_ms,
            last.map(|l| l.income)
        );
    }
    println!("axis {axis} allies {allies} none {none}");
}

fn dump(args: &[String]) {
    let seed: u64 = args[2].parse().unwrap();
    let rounds: u32 = args[3].parse().unwrap();
    let power = Power::from_id(&args[4]).expect("power id");
    let rep = play(
        GameSettings {
            seed,
            ..GameSettings::default()
        },
        [Style::Aggressive; 5],
        Verbosity::Normal,
        seed,
        rounds,
    );
    let st = &rep.state;
    let mut by: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for u in st.units.iter().filter(|u| u.owner == power) {
        let n = wartide_engine::MapData::get().loc(u.location).name.clone();
        by.entry(n).or_default().push(format!(
            "{:?}{}",
            u.kind,
            if u.loaded_on.is_some() { "*" } else { "" }
        ));
    }
    for (k, v) in by {
        println!("{k}: {}", v.join(" "));
    }
    println!("treasury {}", st.treasury[power]);
}
