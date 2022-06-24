use std::{
    collections::{HashMap, HashSet},
    ops::Add,
    sync::Arc,
};

use egg_mode::{entities::MediaEntity, stream::StreamMessage};
use futures::{FutureExt, TryStreamExt};
use log::{error, info, warn};
use r_cache::cache::Cache;
use teloxide::{
    adaptors::{AutoSend, DefaultParseMode},
    payloads::SendMessageSetters,
    prelude::Requester,
    types::{
        InlineKeyboardButton, InlineKeyboardMarkup, InputFile, InputMedia, InputMediaAnimation,
        InputMediaPhoto, InputMediaVideo, UserId,
    },
    utils::markdown::{bold, escape},
    Bot,
};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    RwLock,
};
use url::Url;

use crate::models::{
    blacklist_model::{self, Blacklist},
    follow_model::Follow,
};

struct TwitterTokenContext {
    follows: Vec<u64>,
    end_tx: Option<tokio::sync::oneshot::Sender<()>>,
    token: String,
    user_id: i64,
}

pub struct TwitterSubscriber {
    tg_bot: AutoSend<DefaultParseMode<Bot>>,
    tweet_tx: Sender<StreamMessage>,
    subscribe_tx: Sender<String>,
    token_map: HashMap<String, TwitterTokenContext>,
    token_vec: Vec<String>,
    twitter_sub_to_token_map: HashMap<i64, String>,
    blacklist_map: HashMap<i64, HashSet<(i64, i32)>>,
    follow_map: HashMap<i64, HashSet<i64>>,
    follow_to_twiiter: HashMap<i64, Vec<i64>>,
    block_rt_count_map: HashMap<i64, HashMap<i64, i64>>,
    follow_rt_count_map: HashMap<i64, HashMap<i64, i64>>,
}

impl TwitterSubscriber {
    pub fn new(
        tweet_tx: Sender<StreamMessage>,
        subscribe_tx: Sender<String>,
        tg_bot: AutoSend<DefaultParseMode<Bot>>,
        blacklist_map: HashMap<i64, HashSet<(i64, i32)>>,
    ) -> Self {
        TwitterSubscriber {
            tg_bot,
            tweet_tx,
            subscribe_tx,
            token_map: HashMap::new(),
            twitter_sub_to_token_map: HashMap::new(),
            blacklist_map,
            follow_map: HashMap::new(),
            token_vec: Vec::new(),
            follow_to_twiiter: HashMap::new(),
            block_rt_count_map: HashMap::new(),
            follow_rt_count_map: HashMap::new(),
        }
    }

    fn token_hash(token: &str) -> String {
        format!("{:x}", md5::compute(token))
    }

    pub async fn subscribe_worker(ts: Arc<RwLock<TwitterSubscriber>>, mut rx: Receiver<String>) {
        while let Some(t) = rx.recv().await {
            let _ = TwitterSubscriber::subscribe(ts.clone(), t).await;
        }
    }

    pub async fn check_token_valid(token: &str) -> Result<bool, anyhow::Error> {
        let t: egg_mode::Token = serde_json::from_str(token)?;
        let user = egg_mode::user::show(783214, &t).await?;
        Ok(user.screen_name.eq("Twitter"))
    }

    pub async fn forward_tweet(
        forward_history: Arc<Cache<String, ()>>,
        ts: Arc<RwLock<TwitterSubscriber>>,
        mut tweet_rx: Receiver<StreamMessage>,
    ) {
        while let Some(m) = tweet_rx.recv().await {
            let msg = match m {
                StreamMessage::Tweet(t) => format_tweet(t),
                _ => None,
            };
            if let Some((twitter_user_id, retweet_user_id, tweet_url, msg, media)) = msg {
                let ts_read = ts.read().await;
                let users = match ts_read.follow_to_twiiter.get(&(twitter_user_id as i64)) {
                    Some(users) => users.clone(),
                    None => Vec::new(),
                };
                if users.len().eq(&0) {
                    drop(ts_read);
                    continue;
                }
                let mut tg_user_to_send = Vec::new();
                for tg_user_id in users {
                    // 检查重复推送记录
                    let cache_key = format!(
                        "{:x}",
                        md5::compute(format!("{:?}-{}", tg_user_id, &tweet_url))
                    );
                    if forward_history.get(&cache_key).await.is_some() {
                        continue;
                    }
                    forward_history.set(cache_key, (), None).await;

                    // 检查直推转推黑名单
                    if let Some(blacklist) = ts_read.blacklist_map.get(&(tg_user_id as i64)) {
                        if retweet_user_id.ne(&0) {
                            // 检查转推的 Author 黑名单
                            if blacklist
                                .get(&(
                                    retweet_user_id as i64,
                                    blacklist_model::BlacklistType::BlockTwitter.toi32(),
                                ))
                                .is_some()
                            {
                                continue;
                            }
                            // 检查转推黑名单
                            if blacklist
                                .get(&(
                                    twitter_user_id as i64,
                                    blacklist_model::BlacklistType::BlockRT.toi32(),
                                ))
                                .is_some()
                            {
                                continue;
                            }
                        }
                    }

                    // 添加至通知列表
                    tg_user_to_send.push(tg_user_id);
                }
                let tg = ts_read.tg_bot.clone();
                drop(ts_read);

                for tg_user_id in tg_user_to_send {
                    let markup = InlineKeyboardMarkup::new(vec![
                        get_inline_buttons(tg_user_id, retweet_user_id, &ts, twitter_user_id).await,
                    ]);
                    if !media.is_empty() {
                        let media_group_id = tg
                            .send_media_group(UserId(tg_user_id.clone() as u64), media.clone())
                            .await;
                        if let Err(e) = media_group_id {
                            error!("telegram@{} send_media_group {:?}", &tg_user_id, e);
                        }
                    }
                    let res = tg
                        .send_message(UserId(tg_user_id.clone() as u64), &msg)
                        .reply_markup(markup.clone())
                        .await;
                    if res.is_err() {
                        error!(
                            "telegram@{} send_message {:?}",
                            &tg_user_id,
                            res.err().unwrap()
                        );
                    }
                }
            }
        }
    }

    pub async fn add_follow(
        &mut self,
        f: Follow,
        from_twitter_user_id: i64,
    ) -> Result<String, anyhow::Error> {
        if self.token_vec.len().eq(&0) {
            return Err(anyhow::anyhow!("No valid Twitter token"));
        }

        // 添加到个人订阅列表
        if let Some(list) = self.follow_map.get_mut(&f.user_id) {
            if !list.contains(&f.twitter_user_id) {
                list.insert(f.twitter_user_id);
            }
        } else {
            self.follow_map
                .insert(f.user_id, HashSet::from([f.twitter_user_id]));
        }

        // 优质内容来源计数
        if from_twitter_user_id.gt(&0) {
            if !self.follow_rt_count_map.contains_key(&f.user_id) {
                self.follow_rt_count_map
                    .insert(f.user_id, HashMap::from([(from_twitter_user_id, 1)]));
            } else {
                let brc = self.follow_rt_count_map.get_mut(&f.user_id).unwrap();
                if !brc.contains_key(&from_twitter_user_id) {
                    brc.insert(from_twitter_user_id, 1);
                } else {
                    let count = brc.get_mut(&from_twitter_user_id).unwrap();
                    *count = count.add(1);
                }
            }
        }

        // 检查是否存在于全局订阅列表
        if self
            .twitter_sub_to_token_map
            .contains_key(&f.twitter_user_id)
        {
            return Ok("".to_string());
        }

        // 添加到全局订阅列表
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
        self.twitter_sub_to_token_map
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

    pub fn remove_follow(&mut self, user_id: i64, twitter_id: i64) -> String {
        if !self.follow_to_twiiter.contains_key(&twitter_id) {
            return "".to_string();
        }

        // 删掉订阅关系
        self.follow_map
            .get_mut(&user_id)
            .unwrap()
            .remove(&twitter_id);

        // 从全局订阅记录删掉
        let users = self.follow_to_twiiter.get_mut(&twitter_id).unwrap();
        let index = users.iter().position(|f| f.eq(&user_id)).unwrap();
        users.remove(index);

        // 如果还有其他人订阅直接退出
        if users.len().gt(&0) {
            return "".to_string();
        };

        let hash = self.twitter_sub_to_token_map.get(&twitter_id).unwrap();
        let ctx = self.token_map.get_mut(hash).unwrap();
        let index = ctx
            .follows
            .iter()
            .position(|f| f.eq(&(twitter_id as u64)))
            .unwrap();
        ctx.follows.remove(index);
        self.twitter_sub_to_token_map.remove(&twitter_id);
        ctx.end_tx.take().unwrap().send(()).unwrap();
        ctx.token.clone()
    }

    pub async fn block(
        &mut self,
        b: Blacklist,
        from_twitter_user_id: i64,
    ) -> Result<(), anyhow::Error> {
        // 劣质内容屏蔽计数
        if b.type_.eq(&2) {
            if !self.block_rt_count_map.contains_key(&b.user_id) {
                self.block_rt_count_map
                    .insert(b.user_id, HashMap::from([(from_twitter_user_id, 1)]));
            } else {
                let brc = self.block_rt_count_map.get_mut(&b.user_id).unwrap();
                if !brc.contains_key(&from_twitter_user_id) {
                    brc.insert(from_twitter_user_id, 1);
                } else {
                    let count = brc.get_mut(&from_twitter_user_id).unwrap();
                    *count = count.add(1);
                }
            }
        }

        let list = self.blacklist_map.get_mut(&b.user_id);
        let item = (b.twitter_user_id, b.type_);
        if list.is_none() {
            self.blacklist_map.insert(b.user_id, HashSet::from([item]));
        } else {
            if list.as_ref().unwrap().contains(&item) {
                return Ok(());
            }
            list.unwrap().insert(item);
        }
        Ok(())
    }

    pub async fn unblock(&mut self, user_id: i64, twitter_id: i64, x_type: i32) {
        let list = self.blacklist_map.get_mut(&user_id);
        if let Some(list) = list {
            list.remove(&(twitter_id, x_type));
        }
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
        let mut tokens_need_to_react = HashSet::new();
        for f in follows {
            let mut ts_writer = ts.write().await;
            let using_token = ts_writer.remove_follow(user_id, f as i64);
            drop(ts_writer);
            if using_token.ne("") && using_token.ne(token) {
                tokens_need_to_react.insert(using_token);
            }
        }
        // Reorganizing token subscription relationships
        let ts_read = ts.read().await;
        let subscribe_tx = ts_read.subscribe_tx.clone();
        drop(ts_read);
        for t in tokens_need_to_react {
            let _ = subscribe_tx.send(t).await;
        }
        // 给用户一个通知
        let res = tg_bot
            .send_message(
                UserId(user_id as u64),
                escape(
                    "Your Twitter authorization has expired, you will not receive future messages.",
                ),
            )
            .await;
        if res.is_err() {
            error!("telegram@{} {:?}", &user_id, res.err().unwrap());
        }
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
                info!("Twitter token {:?} subscribe get writer", hash);
                let mut ts_writer = ts.write().await;
                info!("Twitter token {:?} subscribe geted writer", hash);
                let ctx = ts_writer.token_map.get_mut(&hash).unwrap();
                // 停掉之前的 follow 线程
                if let Some(ch) = ctx.end_tx.as_ref() {
                    drop(ch);
                }
                let follows = ctx.follows.clone();
                // 如果此 Token 下没有分配的 follow 了，直接退出
                if follows.is_empty() {
                    info!("Twitter token {:?} no follows exit", hash);
                    return;
                }
                let (tx, rx) = tokio::sync::oneshot::channel::<()>();
                ctx.end_tx = Some(tx);
                drop(ts_writer);
                info!("Twitter {:?} subscribe", &follows);
                let mut stream = egg_mode::stream::filter()
                    .follow(follows.as_slice())
                    .start(&t);
                let mut rx_fuse = rx.fuse();
                loop {
                    tokio::select! {
                       res = stream.try_next() => {
                            match res {
                                Ok(m) => {
                                    if let Some(m) = m {
                                        let ts_read = ts.read().await;
                                        ts_read.tweet_tx.send(m).await.unwrap();
                                    }
                                    continue;
                                },
                                Err(e)=>{
                                    // twitter 的 stream 出错退出，先打印错误信息
                                    warn!("Twitter {:?} subscribe error {:?}", &follows, e);
                                    // 再检查一下 token 有效性，如果确认无效，走删除 token 流程
                                    let res = Self::check_token_valid(&token).await;
                                    if res.is_err() || !res.unwrap() {
                                        let res = Self::remove_token(ts.clone(), &token).await;
                                        if let Err(e) = res {
                                            error!("Twitter {:?} remove token error {:?}", &follows, e);
                                        }
                                        return;
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                                    break;
                                }
                            };
                        },
                        _ = &mut rx_fuse => {
                            error!("Twitter {:?} subscribe active exit", &follows);
                            return;
                       },
                    };
                }
            }
        });
        Ok(())
    }
}

async fn get_inline_buttons(
    tg_user_id: i64,
    retweet_user_id: u64,
    ts: &Arc<RwLock<TwitterSubscriber>>,
    twitter_user_id: u64,
) -> Vec<InlineKeyboardButton> {
    let ts_read = ts.read().await;
    let follow_count = match ts_read.follow_rt_count_map.get(&tg_user_id) {
        Some(m) => m.get(&(twitter_user_id as i64)).unwrap_or(&0).clone(),
        None => 0,
    };
    let block_count = match ts_read.block_rt_count_map.get(&tg_user_id) {
        Some(m) => m.get(&(twitter_user_id as i64)).unwrap_or(&0).clone(),
        None => 0,
    };
    drop(ts_read);

    let mut inline_buttons = Vec::new();
    if retweet_user_id > 0 {
        let ts_read = ts.read().await;
        inline_buttons.push(InlineKeyboardButton::callback(
            "🚫RTer".to_string(),
            format!("/BlockTwitterID 2 {} {}", retweet_user_id, twitter_user_id),
        ));
        if !ts_read
            .follow_map
            .get(&tg_user_id)
            .unwrap()
            .contains(&(retweet_user_id as i64))
        {
            inline_buttons.push(InlineKeyboardButton::callback(
                format!("👀RTer({})", follow_count),
                format!("/FollowTwitterID {} {}", retweet_user_id, twitter_user_id),
            ));
        } else {
            inline_buttons.push(InlineKeyboardButton::callback(
                "❌RT".to_string(),
                format!("/UnfollowTwitterID {}", retweet_user_id),
            ));
        }
        inline_buttons.push(InlineKeyboardButton::callback(
            format!("🚫RT({})", block_count),
            format!("/BlockTwitterID 1 {} {}", twitter_user_id, 0),
        ));
        inline_buttons.push(InlineKeyboardButton::callback(
            "❌".to_string(),
            format!("/UnfollowTwitterID {}", twitter_user_id),
        ));
    } else {
        inline_buttons.push(InlineKeyboardButton::callback(
            "Unfollow".to_string(),
            format!("/UnfollowTwitterID {}", twitter_user_id),
        ));
    }
    inline_buttons
}

fn format_tweet(t: egg_mode::tweet::Tweet) -> Option<(u64, u64, String, String, Vec<InputMedia>)> {
    let user = t.user.as_ref().unwrap();
    let mut caption = user.screen_name.clone();
    let mut retweet_user_id = 0;
    let mut real_created_at = t.created_at;
    let mut tweet_url = format!(
        "https://twitter.com/{}/status/{:?}",
        &user.screen_name, t.id
    );
    if let Some(ts) = t.retweeted_status {
        real_created_at = ts.created_at;
        if let Some(rt) = ts.user {
            caption = rt.screen_name.clone();
            retweet_user_id = rt.id;
            tweet_url = format!("https://twitter.com/{}/status/{:?}", &rt.screen_name, ts.id);
        }
    }

    // 忽略三天前的 tweet
    if real_created_at < chrono::Utc::now() - chrono::Duration::days(3) {
        return None;
    }

    // 忽略自己转发自己的推文
    if user.id.eq(&retweet_user_id) {
        return None;
    };

    let mut media = Vec::new();
    let ext_media: Option<Vec<MediaEntity>> = match t.extended_entities {
        Some(ext) => Some(ext.media),
        None => t.entities.media,
    };
    if let Some(ext_media) = ext_media {
        ext_media.iter().for_each(|m| {
            let m_i = get_media_from_media_entity(m, &caption);
            if let Some(m_i) = m_i {
                media.push(m_i);
            }
        });
    }

    let screen_name_with_count = bold(&escape(&format!("{}", &user.screen_name)));

    let msg = match media.is_empty() {
        true => t.text,
        false => t
            .text
            .replace("https://t.co", "t_co")
            .replace("https://twitter.com", "twitter_com"),
    };

    Some((
        user.id,
        retweet_user_id,
        tweet_url.clone(),
        format!("{}: {}", screen_name_with_count, escape(&msg)),
        media,
    ))
}

fn get_media_from_media_entity(
    m: &egg_mode::entities::MediaEntity,
    caption: &str,
) -> Option<InputMedia> {
    let video_url = get_max_video_bitrate(m);
    if let Some(url) = video_url.1 {
        return Some(InputMedia::Video(
            InputMediaVideo::new(InputFile::url(url.parse().unwrap())).caption(caption),
        ));
    }

    match m.media_type {
        egg_mode::entities::MediaType::Photo => Some(InputMedia::Photo(
            InputMediaPhoto::new(InputFile::url(Url::parse(&m.media_url).unwrap()))
                .caption(caption),
        )),
        egg_mode::entities::MediaType::Gif => Some(InputMedia::Animation(
            InputMediaAnimation::new(InputFile::url(Url::parse(&m.media_url).unwrap()))
                .caption(caption),
        )),
        _ => None,
    }
}

fn get_max_video_bitrate(m: &egg_mode::entities::MediaEntity) -> (i32, Option<String>) {
    if m.video_info.is_none() || m.video_info.as_ref().unwrap().variants.len() == 0 {
        return (0, None);
    }
    let mut variants = m.video_info.as_ref().unwrap().variants.clone();
    variants.sort_by(|v1, v2| v2.bitrate.cmp(&v1.bitrate));
    (
        variants.first().unwrap().bitrate.unwrap(),
        Some(variants.first().unwrap().url.clone()),
    )
}
