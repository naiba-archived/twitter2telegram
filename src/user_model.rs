use crate::schema::users::dsl::*;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::{QueryDsl, Queryable, RunQueryDsl, SqliteConnection};

#[derive(Queryable, Debug, Clone)]
pub struct User {
    pub id: i64,
    pub label: String,
    pub telegram_status: bool,
    pub twitter_access_token: Option<String>,
    pub twitter_status: bool,
    pub created_at: NaiveDateTime,
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
            telegram_status.eq(u.telegram_status),
            twitter_status.eq(u.twitter_status),
        ))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn update_user(conn: &SqliteConnection, u: User) -> Result<usize, anyhow::Error> {
    let res = diesel::update(users)
        .filter(id.eq(u.id))
        .set((
            telegram_status.eq(u.telegram_status),
            twitter_access_token.eq(u.twitter_access_token),
            twitter_status.eq(u.twitter_status),
        ))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}
