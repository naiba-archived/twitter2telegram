pub mod models;
pub mod telegram_bot;
pub mod twitter_subscriber;

pub const GIT_HASH: &'static str = env!("GIT_HASH");

#[macro_use]
extern crate diesel;
