use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::NaiveDateTime;
use egg_mode::KeyPair;
use r_cache::cache::Cache;
use teloxide::{
    adaptors::DefaultParseMode,
    prelude::*,
    types::ParseMode,
    utils::{command::BotCommand, markdown::escape},
};
use tokio::sync::RwLock;

use crate::{
    follow_model,
    twitter_subscriber::TwitterSubscriber,
    user_model::{self, User},
    DbPool,
};

#[derive(BotCommand)]
#[command(
    description = "T2TBot: bot that retweets tweets to telegram, all parameters should be appended to the command, separated by spaces, e.g. `/SetTwitterVerifyCode 1234567`, *BEFORE YOU START*, you should complete step 1 \\-\\-\\> 2\\.\n"
)]
enum Command {
    #[command(rename = "lowercase", description = "Menu")]
    Start,
    #[command(description = "Step1: Get the authorization URL for twitter")]
    GetTwitterAuthURL,
    #[command(
        description = "Step2: Set the Twitter authorisation code _\\(parameter: 7 digits)\\)_"
    )]
    SetTwitterVerifyCode(String),
    #[command(
        description = "Subscribe to [Twitter ID](https://tweeterid.com) _\\(parameter: a huge number\\)_"
    )]
    FollowTwitterID(i64),
    #[command(description = "Unsubscribe from Twitter ID _\\(parameter: a huge number\\)_")]
    UnfollowTwitterID(i64),
    #[command(description = "List subscribed Twitter users")]
    ListFollowedTwitterID,
    #[command(description = "*OWNER* Add a user", parse_with = "split")]
    AddUser {
        telegram_id: i64,
        custom_label: String,
    },
}

pub struct TelegramBot {
    pub bot: AutoSend<DefaultParseMode<teloxide::Bot>>,
    pub name: String,
    pub db_pool: DbPool,
    pub cache: Cache<i64, egg_mode::KeyPair>,
    pub telegram_admin_id: i64,
    pub twitter_token: KeyPair,
    pub twitter_subscriber: Option<Arc<RwLock<TwitterSubscriber>>>,
}

impl TelegramBot {
    pub fn new(
        name: String,
        cache: Cache<i64, egg_mode::KeyPair>,
        db_pool: DbPool,
        telegram_admin_id: i64,
        twitter_token: KeyPair,
        tg_token: String,
    ) -> Self {
        let bot = teloxide::Bot::new(&tg_token)
            .parse_mode(ParseMode::MarkdownV2)
            .auto_send();
        TelegramBot {
            bot: bot,
            name: name,
            cache: cache,
            db_pool: db_pool,
            telegram_admin_id,
            twitter_token: twitter_token,
            twitter_subscriber: None,
        }
    }

    pub fn set_twitter_subscriber(&mut self, subscriber: Option<Arc<RwLock<TwitterSubscriber>>>) {
        self.twitter_subscriber = subscriber;
    }
}

async fn answer(
    tg_bot: Arc<TelegramBot>,
    cx: UpdateWithCx<AutoSend<DefaultParseMode<Bot>>, Message>,
    command: Command,
) -> Result<(), anyhow::Error> {
    let sender = cx.update.from().unwrap();

    let user = match user_model::get_user_by_id(&tg_bot.db_pool.get().unwrap(), sender.id) {
        Ok(u) => Some(u),
        Err(_) => None,
    };

    let user_pre_check = || async {
        if user.is_none() {
            cx.answer(format!(
                "User {:?} not authorized, please contact administrator to add permissions",
                sender.id
            ))
            .await
            .unwrap();
            return false;
        };
        true
    };

    let admin_pre_check = || async {
        if !sender.id.eq(&tg_bot.telegram_admin_id) {
            cx.answer("You are not an admin").await.unwrap();
            return false;
        };
        true
    };

    match command {
        Command::Start => {
            if !user_pre_check().await {
                return Ok(());
            };
            cx.answer(Command::descriptions().replace(" - ", " \\- "))
                .await?
        }
        Command::GetTwitterAuthURL => {
            if !user_pre_check().await {
                return Ok(());
            };
            let request_token = egg_mode::auth::request_token(&tg_bot.twitter_token, "oob")
                .await
                .unwrap();
            let auth_url = egg_mode::auth::authorize_url(&request_token);
            tg_bot
                .cache
                .set(
                    user.unwrap().id,
                    request_token,
                    Some(Duration::from_secs(600)),
                )
                .await;
            cx.answer(auth_url).await?
        }
        Command::SetTwitterVerifyCode(code) => {
            if !user_pre_check().await {
                return Ok(());
            };
            if !code.trim().len().eq(&7) {
                cx.answer(escape("The 7-digit authorization code cannot be empty"))
                    .await?;
                return Ok(());
            }
            let request_token = tg_bot.cache.get(&user.as_ref().unwrap().id).await;
            if request_token.is_none() {
                cx.answer("Please get the Twitter authorization link and authorize first")
                    .await?;
                return Ok(());
            }
            let (token, _, _) = egg_mode::auth::access_token(
                tg_bot.twitter_token.clone(),
                &request_token.unwrap(),
                code,
            )
            .await?;
            let user = user.unwrap();
            let token_str = serde_json::to_string(&token).unwrap();
            let res = user_model::update_user(
                &tg_bot.db_pool.get().unwrap(),
                User {
                    twitter_access_token: Some(token_str.clone()),
                    twitter_status: true,
                    ..user
                },
            );
            let mut ts_write = tg_bot.twitter_subscriber.as_ref().unwrap().write().await;
            ts_write.add_token(&token_str).await?;
            drop(ts_write);
            cx.answer(match res {
                Ok(count) => {
                    format!(
                        "Update Twitter messages successfully, affecting {:?} records",
                        count
                    )
                }
                Err(err) => {
                    format!("Failure, error {:?}", err)
                }
            })
            .await?
        }
        Command::FollowTwitterID(x_twitter_user_id) => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                cx.answer("Please get the Twitter authorization link and authorize first")
                    .await?;
                return Ok(());
            };
            let token: egg_mode::Token =
                serde_json::from_str(&user.twitter_access_token.unwrap()).unwrap();
            let twitter_user = egg_mode::user::show(x_twitter_user_id as u64, &token).await?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let follow = follow_model::Follow {
                id: None,
                user_id: user.id,
                twitter_user_id: x_twitter_user_id,
                twitter_username: twitter_user.screen_name.clone(),
                created_at: NaiveDateTime::from_timestamp(now.as_secs() as i64, now.subsec_nanos()),
            };
            let res = follow_model::create_follow(&tg_bot.db_pool.get().unwrap(), follow.clone());
            cx.answer(match res {
                Ok(count) => {
                    let token = tg_bot
                        .twitter_subscriber
                        .as_ref()
                        .unwrap()
                        .write()
                        .await
                        .add_follow(follow)
                        .await?;
                    if token.ne("") {
                        TwitterSubscriber::subscribe(
                            tg_bot.twitter_subscriber.as_ref().unwrap().clone(),
                            &token,
                        )
                        .await
                        .unwrap();
                    };
                    format!("Added successfully, affecting {:?} records", count)
                }
                Err(err) => {
                    format!("Failure, error {:?}", err)
                }
            })
            .await?
        }
        Command::UnfollowTwitterID(x_twitter_user_id) => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                cx.answer("Please get the Twitter authorization link and authorize first")
                    .await?;
                return Ok(());
            };
            let res =
                follow_model::unfollow(&tg_bot.db_pool.get().unwrap(), user.id, x_twitter_user_id);
            let ts = tg_bot.twitter_subscriber.as_ref().unwrap();
            let mut ts_write = ts.write().await;
            let token = ts_write.remove_follow_id(user.id, x_twitter_user_id);
            drop(ts_write);
            if token.ne("") {
                TwitterSubscriber::subscribe(ts.clone(), &token)
                    .await
                    .unwrap();
            };
            cx.answer(match res {
                Ok(count) => {
                    format!("Unsubscribe Success, affecting {:?} records", count)
                }
                Err(err) => {
                    format!("Failure, error {:?}", err)
                }
            })
            .await?
        }
        Command::ListFollowedTwitterID => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                cx.answer("Please get the Twitter authorization link and authorize first")
                    .await?;
                return Ok(());
            };
            let res = follow_model::get_follows_by_user_id(&tg_bot.db_pool.get().unwrap(), user.id);
            if res.is_err() {
                cx.answer(format!("Failure, error {:?}", res.err())).await?;
                return Ok(());
            }
            let follow_vec = res.unwrap();
            let mut msg = escape("You are currently subscribed to the following accounts.\n");
            follow_vec.iter().for_each(|f| {
                msg.push_str(&format!(
                    "\\* *{}* _{:?}_\n",
                    f.twitter_username, f.twitter_user_id
                ))
            });
            cx.answer(msg).await?
        }
        Command::AddUser {
            telegram_id,
            custom_label,
        } => {
            if !admin_pre_check().await {
                return Ok(());
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let res = user_model::create_user(
                &tg_bot.db_pool.get().unwrap(),
                User {
                    id: telegram_id,
                    label: custom_label.clone(),
                    telegram_status: false,
                    twitter_access_token: None,
                    twitter_status: false,
                    created_at: NaiveDateTime::from_timestamp(
                        now.as_secs() as i64,
                        now.subsec_nanos(),
                    ),
                },
            );
            cx.answer(format!(
                "*{}* _{:?}_ Add {}",
                custom_label.clone(),
                telegram_id,
                match res {
                    Ok(count) => {
                        format!("Success, affecting {:?} Records", count)
                    }
                    Err(err) => {
                        format!("Failure, error {:?}", err)
                    }
                }
            ))
            .await?
        }
    };
    Ok(())
}

pub async fn run(tg_bot: Arc<TelegramBot>) {
    teloxide::commands_repl(
        tg_bot.bot.clone(),
        tg_bot.name.clone(),
        move |cx, command| answer(tg_bot.clone(), cx, command),
    )
    .await;
}
