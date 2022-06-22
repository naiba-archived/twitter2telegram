use std::{
    fmt::Debug,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::NaiveDateTime;
use egg_mode::KeyPair;
use r_cache::cache::Cache;
use teloxide::{
    adaptors::DefaultParseMode,
    prelude::*,
    types::{MessageKind, Update},
    utils::{command::BotCommands, markdown::escape},
};
use tokio::sync::RwLock;

use crate::models::{
    blacklist_model, follow_model,
    user_model::{self, User},
    DbPool,
};
use crate::twitter_subscriber::TwitterSubscriber;
use crate::GIT_HASH;

#[derive(BotCommands, Clone, Debug)]
#[command(description = "T2TBot\\#HASH: bot that retweets tweets to telegram\\.

🚫RTer\\(Block retweets from this retweet's author\\)
👀RTer\\(Follow this retweet's author\\)
❌RT\\(Unfollow retweet's sender\\)
❌\\(Unfollow sender\\)

blockType:
  1 - block all retweets from this user\\.
  2 - block all tweets from this user\\.

usage: */command* _param1_ _param2_")]
enum Command {
    #[command(rename = "lowercase", description = "Menu")]
    Start,
    #[command(description = "Get the authorization URL for twitter")]
    GetTwitterAuthURL,
    #[command(description = "_twitterAuthCode_")]
    SetTwitterVerifyCode(String),
    #[command(
        description = "_twitterID fromTwitterID_ Subscribe to [Twitter ID](https://tweeterid.com)",
        parse_with = "split"
    )]
    FollowTwitterID(i64, i64),
    #[command(description = "_twitterID_ Unsubscribe from Twitter ID")]
    UnfollowTwitterID(i64),
    #[command(
        description = "_blockType twitterID_ Block from Twitter ID",
        parse_with = "split"
    )]
    BlockTwitterID {
        x_type: i32,
        x_twitter_user_id: i64,
        x_from_twitter_id: i64,
    },
    #[command(
        description = "_blockType twitterID fromTwitteID_  Unblock from Twitter ID",
        parse_with = "split"
    )]
    UnblockTwitterID { x_type: i32, x_twitter_user_id: i64 },
    #[command(description = "List block retweet Twitter users")]
    ListBlockedTwitterID(i32),
    #[command(description = "List subscribed Twitter users")]
    ListFollowedTwitterID,
    #[command(description = "*OnlyOwner* Add a user", parse_with = "split")]
    AddUser {
        telegram_id: i64,
        custom_label: String,
    },
}

pub struct TelegramContext {
    pub name: String,
    pub db_pool: DbPool,
    pub cache: Arc<Cache<i64, egg_mode::KeyPair>>,
    pub telegram_admin_id: i64,
    pub twitter_token: KeyPair,
    pub twitter_subscriber: Option<Arc<RwLock<TwitterSubscriber>>>,
}

impl TelegramContext {
    pub fn new(
        name: String,
        cache: Arc<Cache<i64, egg_mode::KeyPair>>,
        db_pool: DbPool,
        telegram_admin_id: i64,
        twitter_token: KeyPair,
    ) -> Self {
        TelegramContext {
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

async fn command_handler(
    ctx: Arc<TelegramContext>,
    bot: AutoSend<DefaultParseMode<Bot>>,
    message: Message,
    command: Command,
) -> Result<(), anyhow::Error> {
    let sender = message.from().unwrap();

    let teloxide::types::UserId(user_id) = sender.id;
    let user = match user_model::get_user_by_id(
        &ctx.db_pool.get().unwrap(),
        user_id.try_into().unwrap(),
    ) {
        Ok(u) => Some(u),
        Err(_) => None,
    };

    let user_pre_check = || async {
        if user.is_none() {
            bot.send_message(
                message.chat.id,
                format!(
                    "User {:?} not authorized, please contact administrator to add permissions",
                    sender.id.0
                ),
            )
            .await
            .unwrap();
            return false;
        };
        true
    };

    let teloxide::types::UserId(sender_id) = sender.id;

    let admin_pre_check = || async {
        if !sender_id.eq(&(ctx.telegram_admin_id as u64)) {
            bot.send_message(message.chat.id, "You are not an admin")
                .await
                .unwrap();
            return false;
        };
        true
    };

    match command {
        Command::Start => {
            if !user_pre_check().await {
                return Ok(());
            };
            bot.send_message(
                message.chat.id,
                Command::descriptions()
                    .to_string()
                    .replace(" - ", " \\- ")
                    .replace("HASH", &GIT_HASH[..8]),
            )
            .await?
        }
        Command::GetTwitterAuthURL => {
            if !user_pre_check().await {
                return Ok(());
            };
            let request_token = egg_mode::auth::request_token(&ctx.twitter_token, "oob")
                .await
                .unwrap();
            let auth_url = egg_mode::auth::authorize_url(&request_token);
            ctx.cache
                .set(
                    user.unwrap().id,
                    request_token,
                    Some(Duration::from_secs(600)),
                )
                .await;
            bot.send_message(message.chat.id, escape(&auth_url)).await?
        }
        Command::SetTwitterVerifyCode(code) => {
            if !user_pre_check().await {
                return Ok(());
            };
            if !code.trim().len().eq(&7) {
                bot.send_message(
                    message.chat.id,
                    escape("The 7-digit authorization code cannot be empty"),
                )
                .await?;
                return Ok(());
            }
            let request_token = ctx.cache.get(&user.as_ref().unwrap().id).await;
            if request_token.is_none() {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            }
            let (token, _, _) = egg_mode::auth::access_token(
                ctx.twitter_token.clone(),
                &request_token.unwrap(),
                code,
            )
            .await?;
            let user = user.unwrap();
            let token_str = serde_json::to_string(&token).unwrap();
            let res = user_model::update_user(
                &ctx.db_pool.get().unwrap(),
                User {
                    twitter_access_token: Some(token_str.clone()),
                    twitter_status: true,
                    ..user
                },
            );
            let mut ts_write = ctx.twitter_subscriber.as_ref().unwrap().write().await;
            ts_write.add_token(user.id, &token_str).await?;
            drop(ts_write);
            bot.send_message(
                message.chat.id,
                match res {
                    Ok(count) => {
                        format!(
                            "Update Twitter messages successfully, affecting {:?} records",
                            count
                        )
                    }
                    Err(err) => {
                        format!("Failure, error {:?}", err)
                    }
                },
            )
            .await?
        }
        Command::FollowTwitterID(x_twitter_user_id, x_from_twitter_user_id) => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            };
            if x_twitter_user_id.le(&0) {
                bot.send_message(message.chat.id, "Incorrect ID").await?;
                return Ok(());
            }
            let token: egg_mode::Token =
                serde_json::from_str(&user.twitter_access_token.unwrap()).unwrap();
            let twitter_user = egg_mode::user::show(x_twitter_user_id as u64, &token).await?;

            // 优质内容追踪计数
            follow_model::increase_follow_rt_count(
                &ctx.db_pool.get().unwrap(),
                user.id,
                x_from_twitter_user_id,
            )?;

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let follow = follow_model::Follow {
                id: None,
                user_id: user.id,
                twitter_user_id: x_twitter_user_id,
                twitter_username: twitter_user.screen_name.clone(),
                follow_rt_count: 0,
                block_rt_count: 0,
                created_at: NaiveDateTime::from_timestamp(now.as_secs() as i64, now.subsec_nanos()),
            };
            let res = follow_model::create_follow(&ctx.db_pool.get().unwrap(), follow.clone());
            bot.send_message(
                message.chat.id,
                match res {
                    Ok(count) => {
                        let token = ctx
                            .twitter_subscriber
                            .as_ref()
                            .unwrap()
                            .write()
                            .await
                            .add_follow(follow, x_twitter_user_id)
                            .await?;
                        if token.ne("") {
                            TwitterSubscriber::subscribe(
                                ctx.twitter_subscriber.as_ref().unwrap().clone(),
                                token,
                            )
                            .await
                            .unwrap();
                        };
                        format!("Added successfully, affecting {:?} records", count)
                    }
                    Err(err) => {
                        format!("Failure, error {:?}", err)
                    }
                },
            )
            .await?
        }
        Command::BlockTwitterID {
            x_type,
            x_twitter_user_id,
            x_from_twitter_id,
        } => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            };
            if x_twitter_user_id.le(&0) {
                bot.send_message(message.chat.id, "Incorrect ID").await?;
                return Ok(());
            }
            let token: egg_mode::Token =
                serde_json::from_str(&user.twitter_access_token.unwrap()).unwrap();
            let twitter_user = egg_mode::user::show(x_twitter_user_id as u64, &token).await?;

            // 劣质内容屏蔽计数
            if x_type.eq(&2) {
                follow_model::increase_block_rt_count(
                    &ctx.db_pool.get().unwrap(),
                    user.id,
                    x_twitter_user_id,
                )?;
            }

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let block = blacklist_model::Blacklist {
                id: None,
                user_id: user.id,
                twitter_user_id: x_twitter_user_id,
                type_: x_type,
                twitter_username: twitter_user.screen_name.clone(),
                created_at: NaiveDateTime::from_timestamp(now.as_secs() as i64, now.subsec_nanos()),
            };
            let res = blacklist_model::block_user(&ctx.db_pool.get().unwrap(), block.clone());

            bot.send_message(
                message.chat.id,
                match res {
                    Ok(count) => {
                        ctx.twitter_subscriber
                            .as_ref()
                            .unwrap()
                            .write()
                            .await
                            .block(block, x_from_twitter_id)
                            .await?;
                        format!("Added successfully, affecting {:?} records", count)
                    }
                    Err(err) => {
                        format!("Failure, error {:?}", err)
                    }
                },
            )
            .await?
        }
        Command::UnblockTwitterID {
            x_type,
            x_twitter_user_id,
        } => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            };
            if x_twitter_user_id.le(&0) {
                bot.send_message(message.chat.id, "Incorrect ID").await?;
                return Ok(());
            }
            let res = blacklist_model::unblock(
                &ctx.db_pool.get().unwrap(),
                user.id,
                x_twitter_user_id,
                x_type,
            );
            bot.send_message(
                message.chat.id,
                match res {
                    Ok(count) => {
                        ctx.twitter_subscriber
                            .as_ref()
                            .unwrap()
                            .write()
                            .await
                            .unblock(user.id, x_twitter_user_id, x_type)
                            .await;
                        format!("Unblock successfully, affecting {:?} records", count)
                    }
                    Err(err) => {
                        format!("Failure, error {:?}", err)
                    }
                },
            )
            .await?
        }
        Command::UnfollowTwitterID(x_twitter_user_id) => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            };
            if x_twitter_user_id.le(&0) {
                bot.send_message(message.chat.id, "Incorrect ID").await?;
                return Ok(());
            }
            let res =
                follow_model::unfollow(&ctx.db_pool.get().unwrap(), user.id, x_twitter_user_id);
            let ts = ctx.twitter_subscriber.as_ref().unwrap();
            let mut ts_write = ts.write().await;
            let token = ts_write.remove_follow(user.id, x_twitter_user_id);
            drop(ts_write);
            if token.ne("") {
                TwitterSubscriber::subscribe(ts.clone(), token)
                    .await
                    .unwrap();
            };
            bot.send_message(
                message.chat.id,
                match res {
                    Ok(count) => {
                        format!("Unsubscribe Success, affecting {:?} records", count)
                    }
                    Err(err) => {
                        format!("Failure, error {:?}", err)
                    }
                },
            )
            .await?
        }
        Command::ListFollowedTwitterID => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            };
            let res = follow_model::get_follows_by_user_id(&ctx.db_pool.get().unwrap(), user.id);
            if res.is_err() {
                bot.send_message(message.chat.id, format!("Failure, error {:?}", res.err()))
                    .await?;
                return Ok(());
            }
            let follow_vec = res.unwrap();
            let mut msg = escape("You are currently subscribed to the following accounts.\n");
            follow_vec.iter().for_each(|f| {
                msg.push_str(&format!(
                    "\\* *{}* _{:?}_\n",
                    escape(&f.twitter_username),
                    f.twitter_user_id
                ))
            });
            bot.send_message(message.chat.id, msg).await?
        }
        Command::ListBlockedTwitterID(x_type) => {
            if !user_pre_check().await {
                return Ok(());
            };
            let user = user.unwrap();
            if !user.twitter_status {
                bot.send_message(
                    message.chat.id,
                    "Please get the Twitter authorization link and authorize first",
                )
                .await?;
                return Ok(());
            };
            let res = blacklist_model::get_blacklist_by_user_id(
                &ctx.db_pool.get().unwrap(),
                user.id,
                x_type,
            );
            if res.is_err() {
                bot.send_message(message.chat.id, format!("Failure, error {:?}", res.err()))
                    .await?;
                return Ok(());
            }
            let list = res.unwrap();
            let mut msg_list: Vec<String> = Vec::new();
            let mut msg = escape("Your blacklist.\n");

            list.chunks(50).for_each(|chunk| {
                chunk.iter().for_each(|f| {
                    msg.push_str(&format!(
                        "\\* {} {:?}\n",
                        escape(&f.twitter_username),
                        f.twitter_user_id
                    ))
                });
                msg_list.push(msg.clone());
                msg.clear();
            });

            for msg in msg_list {
                bot.send_message(message.chat.id, msg).await?;
            }

            return Ok(());
        }
        Command::AddUser {
            telegram_id,
            custom_label,
        } => {
            if !admin_pre_check().await {
                return Ok(());
            }
            if telegram_id.le(&0) {
                bot.send_message(message.chat.id, "Incorrect ID").await?;
                return Ok(());
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let res = user_model::create_user(
                &ctx.db_pool.get().unwrap(),
                User {
                    id: telegram_id,
                    label: custom_label.clone(),
                    twitter_access_token: None,
                    twitter_status: false,
                    created_at: NaiveDateTime::from_timestamp(
                        now.as_secs() as i64,
                        now.subsec_nanos(),
                    ),
                },
            );
            bot.send_message(
                message.chat.id,
                format!(
                    "*{}* _{:?}_ Add {}",
                    escape(&custom_label.clone()),
                    telegram_id,
                    match res {
                        Ok(count) => {
                            format!("Success, affecting {:?} Records", count)
                        }
                        Err(err) => {
                            format!("Failure, error {:?}", err)
                        }
                    }
                ),
            )
            .await?
        }
    };
    Ok(())
}

async fn callback_handler(
    ctx: Arc<TelegramContext>,
    bot: AutoSend<DefaultParseMode<Bot>>,
    q: CallbackQuery,
) -> Result<(), anyhow::Error> {
    if q.data.is_some() && q.message.is_some() {
        if let Ok(cmd) = Command::parse(&q.data.unwrap(), ctx.name.clone()) {
            let mut message = q.message.unwrap();
            if let MessageKind::Common(mut mc) = message.kind.clone() {
                mc.from = Some(q.from);
                message.kind = MessageKind::Common(mc);
            }
            command_handler(ctx, bot, message, cmd).await?;
        }
    }
    Ok(())
}

pub async fn run(bot: AutoSend<DefaultParseMode<Bot>>, tg_ctx: Arc<TelegramContext>) {
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler),
        )
        .branch(Update::filter_callback_query().endpoint(callback_handler));
    log::info!("Tg bot started");
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![tg_ctx.clone()])
        .default_handler(|upd| async move {
            log::warn!("Unhandled update: {:?}", upd);
        })
        .error_handler(LoggingErrorHandler::with_custom_text(
            "An error has occurred in the dispatcher",
        ))
        .build()
        .dispatch()
        .await;
}
