//! Glicko-2 ratings per ladder.
//!
//! Ladders: `public:2p`, `public:5p`, `group:{group_id}:2p`, `group:{group_id}:5p`.
//! Private games without a group are unrated.
//!
//! # Team games
//! Games are always team-vs-team (Axis vs Allies), including 5-player games. [`apply_game_result`]
//! takes the teams ranked best-first (`teams[0]` won). For every player we compute one Glicko-2
//! update per opposing player (win if their team ranked higher, loss if lower) using the
//! pre-game ratings of everybody, then average the resulting (rating, deviation, volatility)
//! over those opponents. So a player is rated "against each opponent, averaged"; teammates do not
//! influence each other. Teams with no human players (bots are never rated) are skipped.
use std::collections::HashMap;

use skillratings::glicko2::{glicko2, Glicko2Config, Glicko2Rating};
use skillratings::Outcomes;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;

pub const DEFAULT_RATING: f64 = 1500.0;

pub fn default_rating() -> Glicko2Rating {
    Glicko2Rating::new()
}

pub fn ladder_for(group: Option<Uuid>, two_player: bool) -> String {
    let suffix = if two_player { "2p" } else { "5p" };
    match group {
        Some(g) => format!("group:{g}:{suffix}"),
        None => format!("public:{suffix}"),
    }
}

/// Pure rating computation. `teams` are ranked best-first; ties are not supported.
/// Returns the new rating for every player (players on a team with no opposing humans are unchanged).
pub fn compute_updates(
    current: &HashMap<Uuid, Glicko2Rating>,
    teams: &[Vec<Uuid>],
) -> HashMap<Uuid, Glicko2Rating> {
    let config = Glicko2Config::new();
    let get = |id: &Uuid| current.get(id).copied().unwrap_or_else(default_rating);
    let mut out = HashMap::new();
    for (i, team) in teams.iter().enumerate() {
        for p in team {
            let me = get(p);
            let mut results = Vec::new();
            for (j, other) in teams.iter().enumerate() {
                if i == j {
                    continue;
                }
                let outcome = if i < j { Outcomes::WIN } else { Outcomes::LOSS };
                for o in other {
                    results.push(glicko2(&me, &get(o), &outcome, &config).0);
                }
            }
            if results.is_empty() {
                out.insert(*p, me);
                continue;
            }
            let n = results.len() as f64;
            out.insert(
                *p,
                Glicko2Rating {
                    rating: results.iter().map(|r| r.rating).sum::<f64>() / n,
                    deviation: results.iter().map(|r| r.deviation).sum::<f64>() / n,
                    volatility: results.iter().map(|r| r.volatility).sum::<f64>() / n,
                },
            );
        }
    }
    out
}

pub async fn get_rating(db: &PgPool, user: Uuid, ladder: &str) -> AppResult<Glicko2Rating> {
    let row: Option<(f64, f64, f64)> = sqlx::query_as(
        "SELECT rating, deviation, volatility FROM ratings WHERE user_id=$1 AND ladder=$2",
    )
    .bind(user)
    .bind(ladder)
    .fetch_optional(db)
    .await?;
    Ok(row
        .map(|(rating, deviation, volatility)| Glicko2Rating {
            rating,
            deviation,
            volatility,
        })
        .unwrap_or_else(default_rating))
}

/// Persist a finished game's result on `ladder`. `teams` ranked best-first, human user ids only.
pub async fn apply_game_result(db: &PgPool, ladder: &str, teams: &[Vec<Uuid>]) -> AppResult<()> {
    // Teams without humans cannot be rated against; drop them but keep ranking order.
    let teams: Vec<Vec<Uuid>> = teams.iter().filter(|t| !t.is_empty()).cloned().collect();
    if teams.len() < 2 {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    let mut current = HashMap::new();
    for p in teams.iter().flatten() {
        // lock rows (insert default if missing)
        sqlx::query("INSERT INTO ratings (user_id, ladder) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(p)
            .bind(ladder)
            .execute(&mut *tx)
            .await?;
        let (rating, deviation, volatility): (f64, f64, f64) = sqlx::query_as(
            "SELECT rating, deviation, volatility FROM ratings WHERE user_id=$1 AND ladder=$2 FOR UPDATE",
        )
        .bind(p)
        .bind(ladder)
        .fetch_one(&mut *tx)
        .await?;
        current.insert(
            *p,
            Glicko2Rating {
                rating,
                deviation,
                volatility,
            },
        );
    }
    let updated = compute_updates(&current, &teams);
    for (i, team) in teams.iter().enumerate() {
        let won = i == 0;
        for p in team {
            let n = updated[p];
            sqlx::query(
                "UPDATE ratings SET rating=$3, deviation=$4, volatility=$5, games=games+1,
                    wins=wins+$6, losses=losses+$7, updated_at=now()
                 WHERE user_id=$1 AND ladder=$2",
            )
            .bind(p)
            .bind(ladder)
            .bind(n.rating)
            .bind(n.deviation)
            .bind(n.volatility)
            .bind(i32::from(won))
            .bind(i32::from(!won))
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::new_v4()).collect()
    }

    #[test]
    fn winner_gains_loser_drops_equal_players() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let out = compute_updates(&HashMap::new(), &[vec![a], vec![b]]);
        assert!(out[&a].rating > 1500.0);
        assert!(out[&b].rating < 1500.0);
        assert!((out[&a].rating - 1500.0 + out[&b].rating - 1500.0).abs() < 1e-6);
        assert!(out[&a].deviation < 350.0);
    }

    #[test]
    fn upset_moves_more_than_expected_win() {
        let (strong, weak) = (Uuid::new_v4(), Uuid::new_v4());
        let mut cur = HashMap::new();
        cur.insert(
            strong,
            Glicko2Rating {
                rating: 1800.0,
                deviation: 80.0,
                volatility: 0.06,
            },
        );
        cur.insert(
            weak,
            Glicko2Rating {
                rating: 1300.0,
                deviation: 80.0,
                volatility: 0.06,
            },
        );
        let expected = compute_updates(&cur, &[vec![strong], vec![weak]]);
        let upset = compute_updates(&cur, &[vec![weak], vec![strong]]);
        let gain_expected = expected[&strong].rating - 1800.0;
        let gain_upset = upset[&weak].rating - 1300.0;
        assert!(gain_upset > gain_expected);
    }

    #[test]
    fn team_game_rates_all_members() {
        let axis = ids(2);
        let allies = ids(3);
        let out = compute_updates(&HashMap::new(), &[axis.clone(), allies.clone()]);
        assert_eq!(out.len(), 5);
        assert!(axis.iter().all(|p| out[p].rating > 1500.0));
        assert!(allies.iter().all(|p| out[p].rating < 1500.0));
        // symmetric within a team
        assert!((out[&axis[0]].rating - out[&axis[1]].rating).abs() < 1e-9);
    }

    #[test]
    fn single_team_unchanged() {
        let a = Uuid::new_v4();
        let out = compute_updates(&HashMap::new(), &[vec![a]]);
        assert_eq!(out[&a].rating, DEFAULT_RATING);
    }

    #[test]
    fn ladder_names() {
        let g = Uuid::nil();
        assert_eq!(ladder_for(None, true), "public:2p");
        assert_eq!(ladder_for(None, false), "public:5p");
        assert_eq!(ladder_for(Some(g), false), format!("group:{g}:5p"));
    }
}
