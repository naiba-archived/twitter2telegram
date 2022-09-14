# T2T Bot

Forward tweets to telegram. (tweets from 3 days ago will be ignored)

|  Menu  |  Tweet  |
|------|------|
|![menu](https://s1.ax1x.com/2022/04/09/LP568f.png)|![tweet](https://s1.ax1x.com/2022/04/09/LPI9G6.png)|

## Commands and Symbols

- **🚫RTer**: If `a` retweeted `b`'s tweet, block `b`'s tweet.
- **👀RTer**: If `a` retweeted `b`'s tweet, follow `b`'s future tweets.
- **❌RT**: Unfollow `b` if `a` retweeted `b`'s tweet and `b` is already being followed.
- **🚫RT**: Disable all tweets retweeted by `a` if `a` retweeted `b`.
- **❌**: Unfollow `a` if `a` posted the tweet.

**Block Commands Parameter** `blockType`:

- **1** \- Block all retweets from this user.
- **2** \- Block all tweets from this user.

## Usage

1. choose a folder to run your bot, like `mkdir some_bot && cd some_bot`
2. create a data dir in your bot folder, `mkdir data`
3. create a docker compose file, `wget https://raw.githubusercontent.com/naiba/twitter2telegram/main/docker-compose.yaml`
4. create a `.env` file, `wget -O .env https://raw.githubusercontent.com/naiba/twitter2telegram/main/.env.example`
5. update twitter/telegram tokens in `.env`
6. run the bot `docker-compose up -d`
7. chat with @userinfobot, get your telegram id
8. add your self as a user, send `/AddUser your_telegram_id a_string_label` to your bot.
