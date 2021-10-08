use std::{env, sync::Arc, time::Duration};

use diesel::{ExpressionMethods, GroupByDsl, QueryDsl, RunQueryDsl};
use dotenv::dotenv;
use r_cache::cache::Cache;
use teloxide::prelude::*;
use tokio::sync::Mutex;
use twitter2telegram::{
    follow_model::Follow, schema::follows::dsl::*, schema::users::dsl::*, telegram_bot,
    twitter_subscriber::TwitterSubscriber, user_model, DbPool,
};
use user_model::User;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let db_pool: twitter2telegram::DbPool =
        twitter2telegram::establish_connection(&env::var("DATABASE_URL").unwrap());
    let cache_instance: Cache<i64, egg_mode::KeyPair> =
        Cache::new(Some(Duration::from_secs(5 * 60)));
    let tg_admin_id: i64 = env::var("ADMIN_ID").unwrap().parse::<i64>().unwrap();
    let twitter_app_token: egg_mode::KeyPair = egg_mode::KeyPair::new(
        env::var("TWITTER_KEY").unwrap(),
        env::var("TWITTER_SECRET").unwrap(),
    );

    let mut tg_bot = telegram_bot::TelegramBot::new(
        "SubscribeTweets".to_string(),
        cache_instance,
        db_pool.clone(),
        tg_admin_id,
        twitter_app_token,
        env::var("TELEGRAM_BOT_TOKEN").unwrap(),
    );

    let twitter_to_tg_bridge = tg_bot.bot.clone();
    let ts = Arc::new(Mutex::new(TwitterSubscriber::new(
        twitter_to_tg_bridge,
        db_pool.clone(),
    )));

    tg_bot.set_twitter_subscriber(Some(ts.clone()));

    tokio::spawn(async {
        run_twitter_subscriber(ts, db_pool).await;
    });

    telegram_bot::run(Arc::new(tg_bot)).await;
}

async fn run_twitter_subscriber(ts: Arc<Mutex<TwitterSubscriber>>, db_pool: DbPool) {
    // 取到所有 twitter token 有效的用户
    let user_vec = users
        .filter(twitter_status.eq(true))
        .load::<User>(&db_pool.get().unwrap())
        .unwrap();
    for u in &user_vec {
        TwitterSubscriber::add_token(ts.clone(), u.twitter_access_token.as_ref().unwrap())
            .await
            .unwrap();
    }
    let user_id_vec = user_vec.iter().map(|u| u.id).collect::<Vec<i64>>();

    // 取到所有有效用户的 follow 的 twitter id
    let follow_vec = follows
        .filter(user_id.eq_any(user_id_vec))
        .group_by(twitter_user_id)
        .load::<Follow>(&db_pool.get().unwrap())
        .unwrap();

    // 加入监听
    let mut ts_w = ts.lock().await;
    for f in follow_vec {
        ts_w.add_follow(f).await.unwrap();
    }

    // 1. 遇到 token 失效的用户，取消监听他们 follow 的 id
    // 2. 遇到新的 token 进入，加入到服务中的 token 列表
    // 3. 遇到新的 follow id，按有效 token 监控的 id 数量正序选择第一个 token 加入监听
    // 4. 遇到取消 follow id，检查这个 id follow 的人数，如果为 0 则取消，否则不做处理
}
