use teloxide::{adaptors::AutoSend, Bot};

use crate::DbPool;

pub struct TwitterSubscriber {
    tg: AutoSend<Bot>,
    db_pool: DbPool,
}

impl TwitterSubscriber {
    pub fn new(tg: AutoSend<Bot>, db_pool: DbPool) -> Self {
        TwitterSubscriber {
            tg: tg,
            db_pool: db_pool,
        }
    }
    pub async fn run() -> Result<(), anyhow::Error> {
        Ok(())
    }
    pub fn add_token(&self, token: String) {}
    pub fn remove_token(&self, token: &str) {}
    pub fn add_follow_id(&self, twitter_id: i64) {}
    pub fn remove_follow_id(&self, twitter_id: i64) {}
}
