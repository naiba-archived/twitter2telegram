use crate::models::schema::follows::dsl::*;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::{QueryDsl, Queryable, RunQueryDsl, SqliteConnection};

#[derive(Clone, Queryable)]
pub struct Follow {
    pub id: Option<i32>,
    pub user_id: i64,
    pub twitter_user_id: i64,
    pub twitter_username: String,
    pub created_at: NaiveDateTime,
    pub follow_rt_count: i64,
    pub block_rt_count: i64,
}

pub fn create_follow(conn: &SqliteConnection, f: Follow) -> Result<usize, anyhow::Error> {
    let res = diesel::insert_into(follows)
        .values((
            user_id.eq(f.user_id),
            twitter_user_id.eq(f.twitter_user_id),
            twitter_username.eq(f.twitter_username),
            created_at.eq(f.created_at),
        ))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn unfollow(
    conn: &SqliteConnection,
    x_user_id: i64,
    x_twitter_user_id: i64,
) -> Result<usize, anyhow::Error> {
    let res = diesel::delete(follows.filter(user_id.eq(x_user_id)))
        .filter(twitter_user_id.eq(x_twitter_user_id))
        .execute(conn);
    match res {
        Ok(cound) => Ok(cound),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn get_follows_by_user_id(
    conn: &SqliteConnection,
    x_user_id: i64,
) -> Result<Vec<Follow>, anyhow::Error> {
    let res = follows.filter(user_id.eq(x_user_id)).load::<Follow>(conn);
    match res {
        Ok(vec) => Ok(vec),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn get_all_follows(conn: &SqliteConnection) -> Result<Vec<Follow>, anyhow::Error> {
    let res = follows.load::<Follow>(conn);
    match res {
        Ok(vec) => Ok(vec),
        Err(e) => Err(anyhow!("{:?}", e)),
    }
}

pub fn increase_block_rt_count(
    conn: &SqliteConnection,
    x_user_id: i64,
    x_twitter_user_id: i64,
) -> Result<usize, anyhow::Error> {
    let res = diesel::update(
        follows
            .filter(user_id.eq(x_user_id))
            .filter(twitter_user_id.eq(x_twitter_user_id)),
    )
    .set(block_rt_count.eq(block_rt_count + 1))
    .execute(conn)?;
    Ok(res)
}

pub fn increase_follow_rt_count(
    conn: &SqliteConnection,
    x_user_id: i64,
    x_twitter_user_id: i64,
) -> Result<usize, anyhow::Error> {
    let res = diesel::update(
        follows
            .filter(user_id.eq(x_user_id))
            .filter(twitter_user_id.eq(x_twitter_user_id)),
    )
    .set(follow_rt_count.eq(follow_rt_count + 1))
    .execute(conn)?;
    Ok(res)
}
