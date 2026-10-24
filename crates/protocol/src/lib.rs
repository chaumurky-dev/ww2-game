//! Wire types shared by the Wartide server and the wasm client. No I/O dependencies.
pub mod api;
pub mod auth;
pub mod chat;
pub mod game;
pub mod group;
pub mod user;
pub mod ws;

pub use api::{ApiError, Paginated};
