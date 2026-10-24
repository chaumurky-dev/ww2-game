use wartide_ai::selfplay::play;
use wartide_ai::{Bot, Persona, Style, Verbosity};
use wartide_engine::state::{Battle, BattleKind, BattleStep, PendingHits, Side};
use wartide_engine::{Action, GameSettings, GameState, MapData, Phase, Power, UnitType};

fn persona(style: Style, verbosity: Verbosity) -> Persona {
    Persona {
        id: "t".into(),
        name: "Tester".into(),
        style,
        verbosity,
    }
}

fn at(power: Power, phase: Phase) -> GameState {
    let mut st = GameState::new(GameSettings::default());
    st.current_power = power;
    st.phase = phase;
    st
}

#[test]
fn purchases_fit_the_treasury_for_every_power_and_style() {
    for style in [Style::Aggressive, Style::Balanced, Style::Cautious] {
        for p in Power::ALL {
            let mut st = at(p, Phase::Purchase);
            st.treasury[p] = 40;
            let mut bot = Bot::new(persona(style, Verbosity::Chatty), p, 7);
            let (a, chat) = bot.next_action(&st).expect("acts");
            let Action::Purchase { items } = a else {
                panic!("{p:?} did not buy: {a:?}");
            };
            let cost: u32 = items.iter().map(|i| i.unit.stats().cost * i.count).sum();
            assert!((30..=40).contains(&cost), "{p:?} spent {cost}");
            assert!(!chat.is_empty());
        }
    }
}

#[test]
fn research_is_only_for_aggressive_bots_with_a_fat_treasury() {
    let mut st = at(Power::Ussr, Phase::DevelopWeapons);
    st.treasury[Power::Ussr] = 60;
    let mut aggr = Bot::new(
        persona(Style::Aggressive, Verbosity::Normal),
        Power::Ussr,
        1,
    );
    assert!(matches!(
        aggr.next_action(&st).unwrap().0,
        Action::BuyResearch { dice: 1 }
    ));
    let mut bal = Bot::new(persona(Style::Balanced, Verbosity::Normal), Power::Ussr, 1);
    assert_eq!(bal.next_action(&st).unwrap().0, Action::EndPhase);
    st.treasury[Power::Ussr] = 8;
    let mut aggr = Bot::new(
        persona(Style::Aggressive, Verbosity::Normal),
        Power::Ussr,
        1,
    );
    assert_eq!(aggr.next_action(&st).unwrap().0, Action::EndPhase);
}

#[test]
fn bot_stays_silent_when_it_is_not_its_turn() {
    let st = GameState::new(GameSettings::default());
    for p in [Power::Germany, Power::Uk, Power::Japan, Power::Usa] {
        let mut bot = Bot::new(Persona::default(), p, 3);
        assert!(bot.next_action(&st).is_none());
    }
}

fn battle_state(hits: u32) -> (GameState, Vec<u32>) {
    let map = MapData::get();
    let loc = map.id_by_name("Ukraine S.S.R.").expect("Ukraine exists");
    let mut st = at(Power::Ussr, Phase::Combat);
    st.units.retain(|u| u.location != loc);
    let inf = st.spawn(UnitType::Infantry, Power::Ussr, loc);
    let arm = st.spawn(UnitType::Armor, Power::Ussr, loc);
    let ftr = st.spawn(UnitType::Fighter, Power::Ussr, loc);
    let def = st.spawn(UnitType::Infantry, Power::Germany, loc);
    st.battle = Some(Battle {
        location: loc,
        kind: BattleKind::Land,
        attacker: Power::Ussr,
        attackers: vec![inf, arm, ftr],
        defenders: vec![def],
        amphibious: false,
        round: 1,
        step: BattleStep::Resolve,
        pending: Some(PendingHits {
            side: Side::Attacker,
            normal: hits,
            sub: 0,
            immediate: false,
        }),
        queued: None,
        marked_att: vec![],
        marked_def: vec![],
        bombarded: true,
        assault_zones: vec![],
    });
    (st, vec![inf, arm, ftr])
}

#[test]
fn casualties_cheapest_first_and_a_land_unit_survives_to_take_the_territory() {
    let mut bot = Bot::new(persona(Style::Balanced, Verbosity::Quiet), Power::Ussr, 1);
    let (st, ids) = battle_state(1);
    let Action::SelectCasualties { units } = bot.next_action(&st).unwrap().0 else {
        panic!("expected casualties")
    };
    assert_eq!(units, vec![ids[0]], "infantry goes first");

    let (st, ids) = battle_state(2);
    let Action::SelectCasualties { units } = bot.next_action(&st).unwrap().0 else {
        panic!("expected casualties")
    };
    assert_eq!(units.len(), 2);
    assert!(units.contains(&ids[0]), "infantry still goes");
    assert!(
        !units.contains(&ids[1]),
        "armor is kept to capture the territory"
    );
    assert!(units.contains(&ids[2]), "the fighter pays instead");
}

#[test]
fn hopeless_attacks_are_called_off() {
    let (mut st, _) = battle_state(0);
    {
        let loc = st.battle.as_ref().unwrap().location;
        // Pile a huge garrison on the defending side.
        let mut ids = vec![];
        for _ in 0..12 {
            ids.push(st.spawn(UnitType::Infantry, Power::Germany, loc));
        }
        let b = st.battle.as_mut().unwrap();
        b.defenders.extend(ids);
        b.pending = None;
        b.step = BattleStep::Choice;
        // Attackers came from Karelia (friendly) so a retreat exists.
        let from = MapData::get().id_by_name("Russia").unwrap();
        let att = b.attackers.clone();
        for id in att {
            st.unit_mut(id).unwrap().combat_from = Some(from);
        }
    }
    let mut bot = Bot::new(persona(Style::Balanced, Verbosity::Quiet), Power::Ussr, 1);
    let a = bot.next_action(&st).unwrap().0;
    assert!(matches!(a, Action::Retreat { .. }), "{a:?}");
}

#[test]
fn chat_respects_verbosity_and_uses_display_names() {
    let seed = 4;
    let s = |v| {
        play(
            GameSettings {
                seed,
                ..GameSettings::default()
            },
            [Style::Aggressive; 5],
            v,
            seed,
            6,
        )
    };
    let count = |rep: &wartide_ai::selfplay::Report, p: Power, round: u32| {
        rep.chat.iter().filter(|c| c.1 == p && c.0 == round).count()
    };
    let quiet = s(Verbosity::Quiet);
    let normal = s(Verbosity::Normal);
    let chatty = s(Verbosity::Chatty);
    for p in Power::ALL {
        for r in 1..=6 {
            assert!(count(&quiet, p, r) <= 1);
            assert!(count(&normal, p, r) <= 2);
            assert!(count(&chatty, p, r) <= 7);
        }
        // Quiet: at most one line per two rounds.
        for r in 1..6 {
            assert!(
                count(&quiet, p, r) + count(&quiet, p, r + 1) <= 1,
                "{p:?} r{r}"
            );
        }
    }
    assert!(quiet.chat.len() < normal.chat.len());
    assert!(normal.chat.len() < chatty.chat.len());
    let banned = ["fuck", "shit", "damn", "bitch", "ass "];
    for (_, _, text) in &chatty.chat {
        let lower = text.to_lowercase();
        assert!(banned.iter().all(|b| !lower.contains(b)), "{text}");
        assert!(!text.contains("sz_") && !text.contains("{"), "{text}");
        assert!(!text.is_empty() && text.len() < 400);
    }
    assert!(chatty.chat.iter().any(|c| c.2.contains("IPC")));
}
