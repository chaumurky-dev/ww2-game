//! In-process pub/sub for websocket fan-out: one broadcast channel per game and per user.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use uuid::Uuid;
use wartide_protocol::ws::ServerMessage;

const CAPACITY: usize = 256;

#[derive(Clone, Default)]
pub struct Hub {
    games: Arc<Mutex<HashMap<Uuid, broadcast::Sender<ServerMessage>>>>,
    users: Arc<Mutex<HashMap<Uuid, broadcast::Sender<ServerMessage>>>>,
}

fn sender(
    map: &Mutex<HashMap<Uuid, broadcast::Sender<ServerMessage>>>,
    id: Uuid,
) -> broadcast::Sender<ServerMessage> {
    map.lock()
        .unwrap()
        .entry(id)
        .or_insert_with(|| broadcast::channel(CAPACITY).0)
        .clone()
}

impl Hub {
    pub fn subscribe_game(&self, game: Uuid) -> broadcast::Receiver<ServerMessage> {
        sender(&self.games, game).subscribe()
    }
    pub fn subscribe_user(&self, user: Uuid) -> broadcast::Receiver<ServerMessage> {
        sender(&self.users, user).subscribe()
    }
    pub fn publish_game(&self, game: Uuid, msg: ServerMessage) {
        let _ = sender(&self.games, game).send(msg);
    }
    pub fn publish_user(&self, user: Uuid, msg: ServerMessage) {
        let _ = sender(&self.users, user).send(msg);
    }
    /// Drop channels nobody listens to.
    pub fn gc(&self) {
        self.games
            .lock()
            .unwrap()
            .retain(|_, s| s.receiver_count() > 0);
        self.users
            .lock()
            .unwrap()
            .retain(|_, s| s.receiver_count() > 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_reaches_subscribers_only_for_that_game() {
        let hub = Hub::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut ra = hub.subscribe_game(a);
        let mut rb = hub.subscribe_game(b);
        hub.publish_game(
            a,
            ServerMessage::GameUpdated {
                game_id: wartide_protocol::game::GameId(a),
                seq: 3,
            },
        );
        assert!(matches!(
            ra.recv().await.unwrap(),
            ServerMessage::GameUpdated { seq: 3, .. }
        ));
        assert!(rb.try_recv().is_err());
    }
}
