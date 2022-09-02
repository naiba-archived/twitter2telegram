use crate::models::schema::users::dsl::*;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::{QueryDsl, Queryable, RunQueryDsl, SqliteConnection};

#[derive(Queryable, Debug, Clone)]
pub struct User {
    pub id: i64,
    pub label: String,
    pub twitter_access_token: Option<String>,
    pub twitter_status: bool,
    pub created_at: NaiveDateTime,
    pub disable_retweet: bool,
    pub disable_text_msg: bool,
}

pub fn get_user_by_id(conn: &SqliteConnection, uid: i64) -> Result<User, anyhow::Error> {
    let res = users.filter(id.eq(uid)).first::<User>(conn);
    match res {
        Ok(u) => Ok(u),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn create_user(conn: &SqliteConnection, u: User) -> Result<usize, anyhow::Error> {
    let res = diesel::insert_into(users)
        .values((
            id.eq(u.id),
            label.eq(u.label),
            created_at.eq(u.created_at),
            twitter_status.eq(u.twitter_status),
        ))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn update_twitter_token(
    conn: &SqliteConnection,
    uid: i64,
    i_twitter_access_token: String,
    i_twitter_status: bool,
) -> Result<usize, anyhow::Error> {
    let res = diesel::update(users)
        .filter(id.eq(uid))
        .set((
            twitter_access_token.eq(i_twitter_access_token),
            twitter_status.eq(i_twitter_status),
        ))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn update_disable_retweet(
    conn: &SqliteConnection,
    uid: i64,
    disable: bool,
) -> Result<usize, anyhow::Error> {
    let res = diesel::update(users)
        .filter(id.eq(uid))
        .set((disable_retweet.eq(disable),))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn update_disable_text_msg(
    conn: &SqliteConnection,
    uid: i64,
    disable: bool,
) -> Result<usize, anyhow::Error> {
    let res = diesel::update(users)
        .filter(id.eq(uid))
        .set((disable_text_msg.eq(disable),))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}
