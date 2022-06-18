use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
    time::Duration,
};

use diesel::{ExpressionMethods, GroupByDsl, QueryDsl, RunQueryDsl};
use dotenv::dotenv;
use egg_mode::stream::StreamMessage;
use log::{error, info};
use r_cache::cache::Cache;
use teloxide::{
    adaptors::{AutoSend, DefaultParseMode},
    prelude::{Requester, RequesterExt},
    types::{ParseMode, UserId},
    utils::markdown::escape,
    Bot,
};
use tokio::sync::{
    mpsc::{self, Sender},
    RwLock,
};

use twitter2telegram::{
    models::{
        blacklist_model, establish_connection,
        follow_model::Follow,
        schema::follows::dsl::*,
        schema::users::dsl::*,
        user_model::{self, User},
        DbPool,
    },
    telegram_bot,
    twitter_subscriber::TwitterSubscriber,
};

#[macro_use]
extern crate diesel_migrations;
embed_migrations!("./migrations");

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init_timed();

    let db_pool: DbPool = establish_connection(&env::var("DATABASE_URL").unwrap());

    // auto migration
    info!(
        "migration {:?}",
        diesel_migrations::run_pending_migrations(&db_pool.get().unwrap())
    );

    let cache_instance: Arc<Cache<i64, egg_mode::KeyPair>> =
        Arc::new(Cache::new(Some(Duration::from_secs(5 * 60))));
    tokio::spawn({
        let cache = Arc::clone(&cache_instance);
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                cache.remove_expired().await;
            }
        }
    });

    let telegram_admin_id: i64 = env::var("TELEGRAM_ADMIN_ID")
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let twitter_app_token: egg_mode::KeyPair = egg_mode::KeyPair::new(
        env::var("TWITTER_KEY").unwrap(),
        env::var("TWITTER_SECRET").unwrap(),
    );

    let bot = teloxide::Bot::new(env::var("TELEGRAM_BOT_TOKEN").unwrap())
        .parse_mode(ParseMode::MarkdownV2)
        .auto_send();

    let mut tg_ctx = telegram_bot::TelegramContext::new(
        "T2TBot".to_string(),
        cache_instance,
        db_pool.clone(),
        telegram_admin_id,
        twitter_app_token,
    );

    let (tx, rx) = mpsc::channel::<StreamMessage>(100);
    let (sub_tx, sub_rx) = mpsc::channel::<String>(100);
    let sub_tx_clone = sub_tx.clone();

    // 加载黑名单列表
    let mut blacklist_map: HashMap<i64, HashSet<(i64, i32)>> = HashMap::new();
    let res = blacklist_model::get_all_blacklist(&db_pool.get().unwrap());
    if let Ok(list) = res {
        for item in list {
            let inner_list = blacklist_map.get_mut(&item.user_id);
            if let Some(inner_list) = inner_list {
                inner_list.insert((item.twitter_user_id, item.type_));
            } else {
                blacklist_map.insert(
                    item.user_id,
                    HashSet::from([(item.twitter_user_id, item.type_)]),
                );
            }
        }
    }

    let ts = Arc::new(RwLock::new(TwitterSubscriber::new(
        tx,
        sub_tx_clone,
        bot.clone(),
        blacklist_map,
    )));

    let ts_clone = ts.clone();
    tokio::spawn(async move { TwitterSubscriber::subscribe_worker(ts_clone, sub_rx).await });

    let ts_clone = ts.clone();
    tg_ctx.set_twitter_subscriber(Some(ts_clone));

    let ts_clone = ts.clone();
    let sub_tx_clone = sub_tx.clone();
    let bot_clone = bot.clone();
    tokio::spawn(async {
        run_twitter_subscriber(bot_clone, sub_tx_clone, ts_clone, db_pool).await;
    });

    let ts_clone = ts.clone();
    let forward_history_cache: Arc<Cache<String, ()>> =
        Arc::new(Cache::new(Some(Duration::from_secs(60 * 60 * 24 * 3))));
    tokio::spawn({
        let cache = Arc::clone(&forward_history_cache);
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60 * 60)).await;
                cache.remove_expired().await;
            }
        }
    });
    tokio::spawn(async move {
        TwitterSubscriber::forward_tweet(forward_history_cache, ts_clone, rx).await
    });

    telegram_bot::run(bot.clone(), Arc::new(tg_ctx)).await;
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
    let mut valid_user_id_vec: Vec<i64> = Vec::new();
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
                let res = tg_bot
                    .send_message(UserId(u.id as u64), escape(&e.to_string()))
                    .await;
                if let Err(err) = res {
                    error!("telegram@{} {:?}", &u.id, &err);
                }
            }
        } else {
            valid_user_id_vec.push(u.id);
        }
    }

    // 取到所有有效用户的 follow 的 twitter id
    let follow_vec = follows
        .filter(user_id.eq_any(valid_user_id_vec))
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
