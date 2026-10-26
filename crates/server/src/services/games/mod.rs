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

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

// ---------- pure helpers ----------

pub fn enum_str<T: Serialize>(t: &T) -> String {
    match serde_json::to_value(t) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

pub fn parse_enum<T: DeserializeOwned>(s: &str) -> AppResult<T> {
    serde_json::from_value(Value::String(s.to_string()))
        .map_err(|_| AppError::Internal(anyhow::anyhow!("bad enum value in db: {s}")))
}

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

    #[test]
    fn enum_roundtrip() {
        assert_eq!(enum_str(&GameMode::TwoPlayer), "two_player");
        assert_eq!(
            parse_enum::<GameStatus>("in_progress").unwrap(),
            GameStatus::InProgress
        );
        assert!(parse_enum::<GameStatus>("nope").is_err());
    }
}
