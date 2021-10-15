pub mod follow_model;
pub mod schema;
pub mod telegram_bot;
pub mod twitter_subscriber;
pub mod user_model;

#[macro_use]
extern crate diesel;

use diesel::{
    r2d2::{ConnectionManager, Pool},
    sqlite::SqliteConnection,
};

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

pub fn establish_connection(database_url: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    Pool::builder()
        .max_size(5)
        .build(manager)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}
