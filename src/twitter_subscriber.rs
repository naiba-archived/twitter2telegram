use std::{collections::HashMap, sync::Arc};

use egg_mode::{stream::StreamMessage, tweet::Tweet};
use futures::{FutureExt, TryStreamExt};
use log::{error, info};
use teloxide::{adaptors::AutoSend, prelude::Requester, Bot};
use tokio::sync::RwLock;

use crate::follow_model::Follow;

struct TwitterTokenContext {
    follows: Vec<u64>,
    end_tx: Option<tokio::sync::oneshot::Sender<()>>,
    token: String,
}

pub struct TwitterSubscriber {
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
    fn get_first_media_url(t: &Tweet) -> String {
        match &t.entities.media {
            Some(media) => match media.first() {
                Some(m) => format!("\nmedia: {}", m.media_url_https.clone()),
                None => "".to_string(),
            },
            None => "".to_string(),
        }
    }
    pub fn new(tweet_tx: tokio::sync::mpsc::Sender<StreamMessage>) -> Self {
        TwitterSubscriber {
            tweet_tx,
            token_map: HashMap::new(),
            follow_map: HashMap::new(),
            token_vec: Vec::new(),
            follow_to_twiiter: HashMap::new(),
        }
    }
    pub async fn forward_tweet(
        ts: Arc<RwLock<TwitterSubscriber>>,
        tg: AutoSend<Bot>,
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
                        drop(ts_read);
                        info!(
                            "forward tweet from {}#{:?} to {:?}",
                            &user.screen_name, &user.id, users
                        );
                        for tg_user_id in users {
                            tg.send_message(
                                tg_user_id.clone(),
                                format!(
                                    "{}({:?}): {}{}\nhttps://twitter.com/{}/status/{:?}",
                                    &user.screen_name,
                                    &user.id,
                                    t.text,
                                    &TwitterSubscriber::get_first_media_url(&t),
                                    &user.screen_name,
                                    t.id
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
    pub async fn add_token(&mut self, token: &str) -> Result<(), anyhow::Error> {
        let hash = TwitterSubscriber::token_hash(token);
        if self.token_map.contains_key(&hash) {
            info!("token 已添加过 {}", token);
            return Ok(());
        }
        let t: egg_mode::Token = serde_json::from_str(token)?;
        let user = egg_mode::user::show(783214, &t).await?;
        if user.screen_name.ne("Twitter") {
            return Err(anyhow::anyhow!("token 已失效"));
        }
        self.token_vec.insert(0, hash.clone());
        self.token_map.insert(
            hash.clone(),
            TwitterTokenContext {
                follows: Vec::new(),
                end_tx: None,
                token: token.to_string(),
            },
        );
        Ok(())
    }
    pub async fn add_follow(&mut self, f: Follow) -> Result<String, anyhow::Error> {
        if self.token_vec.len().eq(&0) {
            return Err(anyhow::anyhow!("无有效 Token"));
        }
        if self.follow_map.contains_key(&f.twitter_user_id) {
            return Ok("".to_string());
        }
        let first = &self.token_map.get(&self.token_vec[0]).unwrap();
        // 检查第 0 个 follow 的 id 是否是 0，如果是直接插入
        let mut minimum_follow_token = self.token_vec[0].clone();
        let mut minimum_follow_count = first.follows.len();
        if first.follows.len() > 0 {
            // 对 follow id 进行分配
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
    pub fn remove_token(&self, token: &str) {}
    pub async fn subscribe(
        ts: Arc<RwLock<TwitterSubscriber>>,
        token: &str,
    ) -> Result<(), anyhow::Error> {
        let t: egg_mode::Token = serde_json::from_str(token)?;
        let hash = Self::token_hash(token);
        tokio::spawn(async move {
            loop {
                let mut ts_writer = ts.write().await;
                let ctx = ts_writer.token_map.get_mut(&hash).unwrap();
                if let Some(ch) = ctx.end_tx.as_ref() {
                    drop(ch);
                }
                let follows = ctx.follows.clone();
                let (tx, rx) = tokio::sync::oneshot::channel::<()>();
                ctx.end_tx = Some(tx);
                drop(ts_writer);
                info!("twitter subscribe {:?}", &follows);
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
                                    error!("twitter {:?} subscribe error {:?}", &follows, e);
                                    break;
                                }
                            };
                        },
                        _ = &mut rx_fuse => {
                            info!("twitter {:?} subscribe stop", &follows);
                            return;
                       },
                    };
                }
            }
        });
        Ok(())
    }
}
