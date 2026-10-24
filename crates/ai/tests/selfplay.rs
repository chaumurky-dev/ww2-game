use wartide_ai::selfplay::{play, play_opts};
use wartide_ai::{Style, Verbosity};
use wartide_engine::GameSettings;

fn settings(seed: u64) -> GameSettings {
    GameSettings {
        seed,
        ..GameSettings::default()
    }
}

#[test]
fn bots_play_a_full_game_without_engine_errors() {
    let rounds = if cfg!(debug_assertions) { 8 } else { 30 };
    let rep = play(
        settings(3),
        [Style::Aggressive; 5],
        Verbosity::Chatty,
        3,
        rounds,
    );
    assert!(
        rep.errors.is_empty(),
        "engine rejected bot actions: {:?}",
        rep.errors
    );
    assert!(!rep.stalled);
    assert!(rep.max_turn_actions < 400, "{}", rep.max_turn_actions);
    // Bots are not passive: incomes move over time.
    let first = &rep.rounds[0].income;
    assert!(rep.rounds.iter().any(|r| &r.income != first));
    // ...and they actually fight.
    let battles: u32 = rep.battles.iter().map(|b| b[0]).sum();
    assert!(battles >= 8, "only {battles} battles");
    assert!(!rep.chat.is_empty());
}

#[test]
fn games_are_deterministic() {
    let a = play(settings(5), [Style::Balanced; 5], Verbosity::Normal, 5, 4);
    let b = play(settings(5), [Style::Balanced; 5], Verbosity::Normal, 5, 4);
    assert_eq!(a.state, b.state);
    assert_eq!(a.actions, b.actions);
    assert_eq!(a.chat, b.chat);
}

#[test]
fn a_stateless_server_can_recreate_the_bot_for_every_call() {
    let rep = play_opts(
        settings(2),
        [Style::Aggressive; 5],
        Verbosity::Normal,
        2,
        5,
        true,
    );
    assert!(!rep.stalled, "{:?}", rep.errors);
    assert!(rep.errors.len() <= 3, "{:?}", rep.errors);
    assert!(rep.state.round >= 5);
}

#[test]
fn all_styles_finish_rounds_cleanly() {
    for (i, style) in [Style::Balanced, Style::Cautious, Style::Aggressive]
        .into_iter()
        .enumerate()
    {
        let seed = 10 + i as u64;
        let rep = play(settings(seed), [style; 5], Verbosity::Quiet, seed, 6);
        assert!(rep.errors.is_empty(), "{style:?}: {:?}", rep.errors);
        assert!(!rep.stalled);
    }
}

/// Long, release-only: aggressive bots usually end the game before round 30 and each whole turn
/// is planned within the time budget.
#[test]
#[cfg(not(debug_assertions))]
fn aggressive_bots_end_games_and_plan_quickly() {
    let mut finished = 0;
    for seed in 1..=6u64 {
        let rep = play(
            settings(seed),
            [Style::Aggressive; 5],
            Verbosity::Normal,
            seed,
            30,
        );
        assert!(rep.errors.is_empty(), "seed {seed}: {:?}", rep.errors);
        assert!(!rep.stalled);
        assert!(
            rep.max_turn_ms < 300.0,
            "seed {seed}: turn took {} ms",
            rep.max_turn_ms
        );
        if rep.state.winner.is_some() {
            finished += 1;
        } else {
            let first = &rep.rounds[0].income;
            assert!(rep.rounds.iter().any(|r| &r.income != first));
        }
    }
    assert!(finished >= 2, "only {finished}/6 games finished");
}

/// Wider sweep over settings and mixed personas (run with `--ignored --release`).
#[test]
#[ignore]
fn stress_many_seeds_and_settings() {
    let styles = [Style::Aggressive, Style::Balanced, Style::Cautious];
    for seed in 100..130u64 {
        let mut s = settings(seed);
        s.soviet_first_turn_restriction = seed % 3 == 0;
        s.original_complexes_only = seed % 4 == 0;
        s.weapons_development = seed % 5 != 0;
        s.germany_jet_power = seed % 7 == 0;
        s.japan_super_subs = seed % 7 == 1;
        let mix: [Style; 5] = std::array::from_fn(|i| styles[(seed as usize + i) % 3]);
        let rep = play_opts(s, mix, Verbosity::Chatty, seed, 20, seed % 2 == 0);
        assert!(!rep.stalled, "seed {seed}: {:?}", rep.errors);
        assert!(rep.errors.is_empty(), "seed {seed}: {:?}", rep.errors);
    }
}
