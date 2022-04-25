use crate::schema::blacklists::dsl::*;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::{Queryable, RunQueryDsl, SqliteConnection};

pub enum BlacklistType {
    BlockRT,
    BlockTwitter,
}

impl BlacklistType {
    pub fn toi32(&self) -> i32 {
        return match self {
            BlacklistType::BlockRT => 1,
            BlacklistType::BlockTwitter => 2,
        };
    }
}

#[derive(Clone, Queryable)]
pub struct Blacklist {
    pub id: Option<i32>,
    pub user_id: i64,
    pub twitter_user_id: i64,
    pub twitter_username: String,
    pub created_at: NaiveDateTime,
    pub type_: i32,
}

pub fn unblock(
    conn: &SqliteConnection,
    x_user_id: i64,
    x_twitter_user_id: i64,
    x_type: i32,
) -> Result<usize, anyhow::Error> {
    let res = diesel::delete(blacklists.filter(user_id.eq(x_user_id)))
        .filter(twitter_user_id.eq(x_twitter_user_id))
        .filter(type_.eq(x_type))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn block_user(conn: &SqliteConnection, b: Blacklist) -> Result<usize, anyhow::Error> {
    let res = diesel::insert_into(blacklists)
        .values((
            user_id.eq(b.user_id),
            twitter_user_id.eq(b.twitter_user_id),
            twitter_username.eq(b.twitter_username),
            created_at.eq(b.created_at),
            type_.eq(b.type_),
        ))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn get_all_blacklist(conn: &SqliteConnection) -> Result<Vec<Blacklist>, anyhow::Error> {
    let res = blacklists.load::<Blacklist>(conn);
    match res {
        Ok(vec) => Ok(vec),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn get_blacklist_by_user_id(
    conn: &SqliteConnection,
    x_user_id: i64,
    x_type: i32,
) -> Result<Vec<Blacklist>, anyhow::Error> {
    let res = blacklists
        .filter(user_id.eq(x_user_id))
        .filter(type_.eq(x_type))
        .load::<Blacklist>(conn);
    match res {
        Ok(vec) => Ok(vec),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}
