use std::{env, sync::Arc, time::Duration};

use diesel::{ExpressionMethods, GroupByDsl, QueryDsl, RunQueryDsl};
use dotenv::dotenv;
use egg_mode::stream::StreamMessage;
use log::{error, info};
use r_cache::cache::Cache;
use teloxide::{
    adaptors::{AutoSend, DefaultParseMode},
    prelude::Requester,
    utils::markdown::escape,
    Bot,
};
use tokio::sync::{
    mpsc::{self, Sender},
    RwLock,
};

use twitter2telegram::{
    follow_model::Follow, schema::follows::dsl::*, schema::users::dsl::*, telegram_bot,
    twitter_subscriber::TwitterSubscriber, user_model, DbPool,
};
use user_model::User;

#[macro_use]
extern crate diesel_migrations;
embed_migrations!("./migrations");

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init_timed();

    let db_pool: twitter2telegram::DbPool =
        twitter2telegram::establish_connection(&env::var("DATABASE_URL").unwrap());

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "migration" {
        info!(
            "migration {:?}",
            diesel_migrations::run_pending_migrations(&db_pool.get().unwrap())
        );
        return;
    }

    let cache_instance: Cache<i64, egg_mode::KeyPair> =
        Cache::new(Some(Duration::from_secs(5 * 60)));
    let telegram_admin_id: i64 = env::var("TELEGRAM_ADMIN_ID")
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let twitter_app_token: egg_mode::KeyPair = egg_mode::KeyPair::new(
        env::var("TWITTER_KEY").unwrap(),
        env::var("TWITTER_SECRET").unwrap(),
    );

    let mut tg_bot = telegram_bot::TelegramBot::new(
        "T2TBot".to_string(),
        cache_instance,
        db_pool.clone(),
        telegram_admin_id,
        twitter_app_token,
        env::var("TELEGRAM_BOT_TOKEN").unwrap(),
    );

    let bot_clone = tg_bot.bot.clone();
    let (tx, rx) = mpsc::channel::<StreamMessage>(100);
    let (sub_tx, sub_rx) = mpsc::channel::<String>(100);
    let sub_tx_clone = sub_tx.clone();
    let ts = Arc::new(RwLock::new(TwitterSubscriber::new(
        tx,
        sub_tx_clone,
        bot_clone,
    )));

    let ts_clone = ts.clone();
    tokio::spawn(async move { TwitterSubscriber::subscribe_worker(ts_clone, sub_rx).await });

    let ts_clone = ts.clone();
    tg_bot.set_twitter_subscriber(Some(ts_clone));

    let bot_clone = tg_bot.bot.clone();
    let ts_clone = ts.clone();
    let sub_tx_clone = sub_tx.clone();
    tokio::spawn(async {
        run_twitter_subscriber(bot_clone, sub_tx_clone, ts_clone, db_pool).await;
    });

    let ts_clone = ts.clone();
    tokio::spawn(async move { TwitterSubscriber::forward_tweet(ts_clone, rx).await });

    telegram_bot::run(Arc::new(tg_bot)).await;
}

async fn run_twitter_subscriber(
    tg_bot: AutoSend<DefaultParseMode<Bot>>,
    sub_tx: Sender<String>,
    ts: Arc<RwLock<TwitterSubscriber>>,
    db_pool: DbPool,
) {
    // 取到所有 twitter token 有效的用户
    let user_vec = users
        .filter(twitter_status.eq(true))
        .load::<User>(&db_pool.get().unwrap())
        .unwrap();
    let mut ts_writer = ts.write().await;
    for u in &user_vec {
        if let Err(e) = ts_writer
            .add_token(u.id, u.twitter_access_token.as_ref().unwrap())
            .await
        {
            error!("add twitter token: {:?}", e);
            if e.to_string().contains("expired") {
                user_model::update_user(
                    &db_pool.get().unwrap(),
                    User {
                        twitter_status: false,
                        ..u.clone()
                    },
                )
                .unwrap();
                let res = tg_bot.send_message(u.id, escape(&e.to_string())).await;
                if let Err(err) = res {
                    error!("telegram@{} {:?}", &u.id, &err);
                }
            }
        }
    }
    let user_id_vec = user_vec.iter().map(|u| u.id).collect::<Vec<i64>>();

    // 取到所有有效用户的 follow 的 twitter id
    let follow_vec = follows
        .filter(user_id.eq_any(user_id_vec))
        .group_by(twitter_user_id)
        .load::<Follow>(&db_pool.get().unwrap())
        .unwrap();
    drop(ts_writer);

    // 加入监听
    let mut ts_writer2 = ts.write().await;
    for f in follow_vec {
        ts_writer2.add_follow(f).await.unwrap();
    }
    drop(ts_writer2);

    // 更新监控
    for u in &user_vec {
        sub_tx
            .send(u.twitter_access_token.as_ref().unwrap().clone())
            .await
            .unwrap();
    }
}
