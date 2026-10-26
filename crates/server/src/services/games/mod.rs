mod ending;
mod lobby;
mod play;
mod read;
mod rows;

pub use ending::*;
pub use lobby::*;
pub use play::*;
use read::clean_game_name;
pub use read::*;
pub use rows::*;
use rows::{GAME_COLS, SEAT_SELECT};

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_protocol::game::*;

    #[test]
    fn layouts() {
        assert_eq!(preset_layout(GameMode::TwoPlayer.seat_count()).len(), 2);
        let five = preset_layout(GameMode::FivePlayer.seat_count());
        assert_eq!(five.len(), 5);
        assert_eq!(five[0], vec![Power::Ussr]);
        let two = preset_layout(GameMode::TwoPlayer.seat_count());
        let total: usize = two.iter().map(Vec::len).sum();
        assert_eq!(total, 5);
        assert!(two[0].iter().all(|p| p.team() == Team::Axis));
    }

    fn seats_for(mode: GameMode) -> Vec<SeatRow> {
        preset_layout(mode.seat_count())
            .into_iter()
            .enumerate()
            .map(|(i, powers)| SeatRow {
                seat: i as i32,
                powers: powers.iter().map(|p| p.as_str().to_string()).collect(),
                ..SeatRow::default()
            })
            .collect()
    }

    #[test]
    fn seat_controls_powers_two_player() {
        let seats = seats_for(GameMode::TwoPlayer);
        for p in [Power::Germany, Power::Japan] {
            assert_eq!(seat_for_power(&seats, p).unwrap().seat, 0);
        }
        for p in [Power::Ussr, Power::Uk, Power::Usa] {
            assert_eq!(seat_for_power(&seats, p).unwrap().seat, 1);
        }
        assert_eq!(seats[0].team(), Team::Axis);
        assert_eq!(seats[1].team(), Team::Allies);
    }

    #[test]
    fn seat_controls_powers_five_player() {
        let seats = seats_for(GameMode::FivePlayer);
        for (i, p) in Power::ALL.into_iter().enumerate() {
            assert_eq!(seat_for_power(&seats, p).unwrap().seat, i as i32);
            assert!(seats.iter().filter(|s| s.controls(p)).count() == 1);
        }
    }

    /// The bytes sqlx writes for a TEXT-backed enum must equal its serde (JSON) string form.
    fn assert_sqlx_matches_serde<T>(variants: &[T])
    where
        T: serde::Serialize + for<'q> sqlx::Encode<'q, sqlx::Postgres>,
    {
        for v in variants {
            let mut buf = sqlx::postgres::PgArgumentBuffer::default();
            let _ = sqlx::Encode::<sqlx::Postgres>::encode_by_ref(v, &mut buf).unwrap();
            let json = serde_json::to_value(v).unwrap();
            assert_eq!(std::str::from_utf8(&buf).unwrap(), json.as_str().unwrap());
        }
    }

    #[test]
    fn enum_roundtrip() {
        assert_sqlx_matches_serde(&[GameMode::TwoPlayer, GameMode::FivePlayer, GameMode::Custom]);
        assert_sqlx_matches_serde(&[GameKind::Private, GameKind::Public]);
        assert_sqlx_matches_serde(&[
            GameStatus::Lobby,
            GameStatus::InProgress,
            GameStatus::Finished,
            GameStatus::Abandoned,
        ]);
        assert_sqlx_matches_serde(&[
            wartide_protocol::chat::ChatChannel::All,
            wartide_protocol::chat::ChatChannel::Team,
            wartide_protocol::chat::ChatChannel::Power,
            wartide_protocol::chat::ChatChannel::Lobby,
        ]);
        for st in [
            GameStatus::Lobby,
            GameStatus::InProgress,
            GameStatus::Finished,
            GameStatus::Abandoned,
        ] {
            assert_eq!(GameStatus::parse(st.as_str()), Some(st));
        }
        for t in [Team::Allies, Team::Axis] {
            assert_eq!(serde_json::to_value(t).unwrap().as_str(), Some(t.as_str()));
            assert_eq!(Team::from_id(t.as_str()), Some(t));
        }
    }
}
