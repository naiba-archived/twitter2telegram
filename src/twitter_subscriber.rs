use std::{collections::HashMap, sync::Arc};

use egg_mode::stream::StreamMessage;
use futures::{FutureExt, TryStreamExt};
use log::{debug, warn};
use teloxide::{
    adaptors::{AutoSend, DefaultParseMode},
    prelude::Requester,
    utils::markdown::{bold, escape, link},
    Bot,
};
use tokio::sync::RwLock;

use crate::follow_model::Follow;

struct TwitterTokenContext {
    follows: Vec<u64>,
    end_tx: Option<tokio::sync::oneshot::Sender<()>>,
    token: String,
    user_id: i64,
}

pub struct TwitterSubscriber {
    tg_bot: AutoSend<DefaultParseMode<Bot>>,
    tweet_tx: tokio::sync::mpsc::Sender<StreamMessage>,
    token_map: HashMap<String, TwitterTokenContext>,
    token_vec: Vec<String>,
    follow_map: HashMap<i64, String>,
    follow_to_twiiter: HashMap<i64, Vec<i64>>,
}

impl TwitterSubscriber {
    fn token_hash(token: &str) -> String {
        format!("{:x}", md5::compute(token))
    }
    pub fn new(
        tweet_tx: tokio::sync::mpsc::Sender<StreamMessage>,
        tg_bot: AutoSend<DefaultParseMode<Bot>>,
    ) -> Self {
        TwitterSubscriber {
            tg_bot,
            tweet_tx,
            token_map: HashMap::new(),
            follow_map: HashMap::new(),
            token_vec: Vec::new(),
            follow_to_twiiter: HashMap::new(),
        }
    }
    pub async fn check_token_valid(token: &str) -> Result<bool, anyhow::Error> {
        let t: egg_mode::Token = serde_json::from_str(token)?;
        let user = egg_mode::user::show(783214, &t).await?;
        Ok(user.screen_name.eq("Twitter"))
    }
    pub async fn forward_tweet(
        ts: Arc<RwLock<TwitterSubscriber>>,
        mut tweet_rx: tokio::sync::mpsc::Receiver<StreamMessage>,
    ) {
        tokio::spawn(async move {
            while let Some(m) = tweet_rx.recv().await {
                match m {
                    StreamMessage::Tweet(t) => {
                        let user = t.user.as_ref().unwrap();
                        let ts_read = ts.read().await;
                        let users = match ts_read.follow_to_twiiter.get(&(user.id as i64)) {
                            Some(users) => users.clone(),
                            None => Vec::new(),
                        };
                        let tg = ts_read.tg_bot.clone();
                        drop(ts_read);
                        debug!(
                            "Forward tweet from {}#{:?} to {:?}",
                            &user.screen_name, &user.id, users
                        );
                        for tg_user_id in users {
                            tg.send_message(
                                tg_user_id.clone(),
                                format!(
                                    "{}: {}",
                                    bold(&escape(&user.screen_name)),
                                    link(
                                        &format!(
                                            "https://twitter.com/{}/status/{:?}",
                                            &user.screen_name, t.id
                                        ),
                                        "credit"
                                    )
                                ),
                            )
                            .await
                            .unwrap();
                        }
                    }
                    _ => {}
                }
            }
        });
    }
    pub async fn add_token(&mut self, user_id: i64, token: &str) -> Result<(), anyhow::Error> {
        let hash = Self::token_hash(token);
        if self.token_map.contains_key(&hash) {
            warn!("Token has been added {}", token);
            return Ok(());
        }
        if !Self::check_token_valid(token).await? {
            return Err(anyhow::anyhow!("Twitter authorization has expired"));
        }
        self.token_vec.insert(0, hash.clone());
        self.token_map.insert(
            hash.clone(),
            TwitterTokenContext {
                user_id,
                follows: Vec::new(),
                end_tx: None,
                token: token.to_string(),
            },
        );
        Ok(())
    }
    pub async fn add_follow(&mut self, f: Follow) -> Result<String, anyhow::Error> {
        if self.token_vec.len().eq(&0) {
            return Err(anyhow::anyhow!("No valid Twitter token"));
        }
        if self.follow_map.contains_key(&f.twitter_user_id) {
            return Ok("".to_string());
        }
        let first = &self.token_map.get(&self.token_vec[0]).unwrap();
        // 检查第 0 个 follow 的 id 是否是 0，如果是直接插入
        let mut minimum_follow_token = self.token_vec[0].clone();
        let mut minimum_follow_count = first.follows.len();
        if first.follows.len() > 0 {
            // 将 follow 分配给 token
            for t in &self.token_vec {
                let count = self.token_map.get(t).unwrap().follows.len();
                if count.lt(&minimum_follow_count) {
                    minimum_follow_count = count;
                    minimum_follow_token = t.to_string();
                    if minimum_follow_count == 0 {
                        break;
                    }
                }
            }
        }
        self.follow_map
            .insert(f.twitter_user_id, minimum_follow_token.clone());
        let minimum = self.token_map.get_mut(&minimum_follow_token).unwrap();
        minimum.follows.push(f.twitter_user_id as u64);
        let followers = self.follow_to_twiiter.get_mut(&f.twitter_user_id);
        if followers.is_none() {
            self.follow_to_twiiter
                .insert(f.twitter_user_id, vec![f.user_id]);
        } else {
            followers.unwrap().push(f.user_id);
        }
        Ok(minimum.token.clone())
    }
    pub fn remove_follow_id(&mut self, user_id: i64, twitter_id: i64) -> String {
        let users = self.follow_to_twiiter.get_mut(&twitter_id).unwrap();
        let index = users.iter().position(|f| f.eq(&user_id)).unwrap();
        users.remove(index);
        if users.len().gt(&0) {
            return "".to_string();
        };
        let hash = self.follow_map.get(&twitter_id).unwrap();
        let ctx = self.token_map.get_mut(hash).unwrap();
        let index = ctx
            .follows
            .iter()
            .position(|f| f.eq(&(twitter_id as u64)))
            .unwrap();
        ctx.follows.remove(index);
        self.follow_map.remove(&twitter_id);
        ctx.end_tx.take().unwrap().send(()).unwrap();
        ctx.token.clone()
    }
    pub async fn remove_token(
        ts: Arc<RwLock<TwitterSubscriber>>,
        token: &str,
    ) -> Result<(), anyhow::Error> {
        let mut ts_writer = ts.write().await;
        let tg_bot = ts_writer.tg_bot.clone();
        let hash = Self::token_hash(&token);
        let ctx = ts_writer.token_map.get_mut(&hash).unwrap();
        // 停掉 token 订阅
        if let Some(ch) = ctx.end_tx.as_ref() {
            drop(ch);
        }
        // 逐个将用户订阅的 twitter 暂停
        let follows = ctx.follows.clone();
        let user_id = ctx.user_id;
        drop(ctx);
        for f in follows {
            let mut ts_writer = ts.write().await;
            let using_token = ts_writer.remove_follow_id(user_id, f as i64);
            drop(ts_writer);
            if using_token.ne("") && using_token.ne(token) {
                // TODO fix cycle call function
                // TwitterSubscriber::subscribe(ts.clone(), using_token).await;
            }
        }
        // 给用户一个通知
        tg_bot
            .send_message(
                user_id,
                escape(
                    "Your Twitter authorization has expired, you will not receive future messages.",
                ),
            )
            .await?;
        Ok(())
    }
    pub async fn subscribe(
        ts: Arc<RwLock<TwitterSubscriber>>,
        token: String,
    ) -> Result<(), anyhow::Error> {
        let t: egg_mode::Token = serde_json::from_str(&token)?;
        let hash = Self::token_hash(&token);
        tokio::spawn(async move {
            loop {
                let mut ts_writer = ts.write().await;
                let ctx = ts_writer.token_map.get_mut(&hash).unwrap();
                // 停掉之前的 follow 线程
                if let Some(ch) = ctx.end_tx.as_ref() {
                    drop(ch);
                }
                let follows = ctx.follows.clone();
                // 如果此 Token 下没有分配的 follow 了，直接退出
                if follows.is_empty() {
                    return;
                }
                let (tx, rx) = tokio::sync::oneshot::channel::<()>();
                ctx.end_tx = Some(tx);
                drop(ts_writer);
                debug!("Twitter subscribe {:?}", &follows);
                let mut stream = egg_mode::stream::filter()
                    .follow(follows.as_slice())
                    .start(&t);
                let mut rx_fuse = rx.fuse();
                loop {
                    tokio::select! {
                       res = stream.try_next() => {
                            match res {
                                Ok(m) => {
                                    let ts_read = ts.read().await;
                                    ts_read.tweet_tx.send(m.unwrap()).await.unwrap();
                                    continue;
                                },
                                Err(e)=>{
                                    // twitter 的 stream 出错退出，先打印错误信息
                                    warn!("Twitter {:?} subscribe error {:?}", &follows, e);
                                    // 再检查一下 token 有效性，如果确认无效，走删除 token 流程
                                    let res = Self::check_token_valid(&token).await;
                                    if res.is_err() || !res.unwrap() {
                                        // TODO check err
                                        let _ = Self::remove_token(ts.clone(), &token).await;
                                        return;
                                    }
                                    break;
                                }
                            };
                        },
                        _ = &mut rx_fuse => {
                            debug!("Twitter {:?} subscribe exited", &follows);
                            return;
                       },
                    };
                }
            }
        });
        Ok(())
    }
}
