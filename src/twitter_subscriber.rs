use std::{collections::HashMap, sync::Arc, time::Duration};

use egg_mode::stream::StreamMessage;
use futures::TryStreamExt;
use teloxide::{adaptors::AutoSend, Bot};
use tokio::sync::{mpsc, Mutex};

use crate::{follow_model::Follow, DbPool};

struct TwitterTokenContext {
    follows: Vec<u64>,
    tx: mpsc::Sender<i64>,
}

pub struct TwitterSubscriber {
    tg: AutoSend<Bot>,
    db_pool: DbPool,
    token_map: HashMap<String, TwitterTokenContext>,
    token_vec: Vec<String>,
    follow_map: HashMap<i64, String>,
}

impl TwitterSubscriber {
    pub fn new(tg: AutoSend<Bot>, db_pool: DbPool) -> Self {
        TwitterSubscriber {
            tg,
            db_pool,
            token_map: HashMap::new(),
            follow_map: HashMap::new(),
            token_vec: Vec::new(),
        }
    }
    pub async fn run() -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn token_hash(token: &str) -> String {
        format!("{:x}", md5::compute(token))
    }
    pub async fn add_token(
        ts_lock: Arc<Mutex<TwitterSubscriber>>,
        token: &str,
    ) -> Result<(), anyhow::Error> {
        let hash = TwitterSubscriber::token_hash(token);
        let mut ts = ts_lock.clone().lock_owned().await;
        if ts.token_map.contains_key(&hash) {
            return Err(anyhow::anyhow!("token 已添加过"));
        }

        ts.token_vec.insert(0, hash.clone());
        let (tx, mut rx) = mpsc::channel::<i64>(1);

        ts.token_map.insert(
            hash.clone(),
            TwitterTokenContext {
                follows: Vec::new(),
                tx,
            },
        );

        drop(ts);
        let hash_clone = hash.clone();
        let t: egg_mode::Token = serde_json::from_str(token)?;
        tokio::spawn(async move {
            while let Some(k) = rx.recv().await {
                let mut ts = ts_lock.clone().lock_owned().await;
                ts.follow_map.insert(k, hash_clone.clone());
                let map = ts.token_map.get_mut(&hash_clone).unwrap();
                map.follows.push(k as u64);
                let follows = map.follows.clone();
                drop(ts);
                loop {
                    let stream = egg_mode::stream::filter()
                        .follow(follows.as_slice())
                        .start(&t)
                        .try_for_each(|m| {
                            if let StreamMessage::Tweet(tweet) = m {
                                println!("tweet {:?}", tweet);
                                println!("──────────────────────────────────────");
                            } else {
                                println!("other {:?}", m);
                            }
                            futures::future::ok(())
                        });
                    if let Err(e) = stream.await {
                        println!("Stream error: {}, disconnected.", e);
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });
        Ok(())
    }

    pub fn remove_token(&self, token: &str) {}
    pub async fn add_follow(&mut self, f: Follow) -> Result<(), anyhow::Error> {
        if self.token_vec.len().eq(&0) {
            return Err(anyhow::anyhow!("无有效 Token"));
        }
        let mut minimum_follow_token = "";
        let mut minimum_follow_count: usize = 0;
        let first = &self.token_map.get(&self.token_vec[0]).unwrap();
        if first.follows.len() == 0 {
            // 检查第 0 个 follow 的 id 是否是 0，如果是直接插入
            minimum_follow_token = &self.token_vec[0]
        } else {
            // 对 follow id 进行分配
            self.token_vec.iter().for_each(|t| {
                let count = self.token_map.get(t).unwrap().follows.len();
                if count.lt(&minimum_follow_count) {
                    minimum_follow_count = count;
                    minimum_follow_token = t
                }
            });
        }
        let minimum = self.token_map.get_mut(minimum_follow_token).unwrap();
        minimum.follows.push(f.id.unwrap() as u64);
        minimum.tx.send(f.id.unwrap() as i64).await?;
        Ok(())
    }
    pub fn remove_follow_id(&self, twitter_id: i64) {}
}
