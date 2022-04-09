# T2T Bot

Forward tweets to telegram.

|  Menu  |  Tweet  |
|------|------|
|![menu](https://s1.ax1x.com/2022/04/09/LP568f.png)|![tweet](https://s1.ax1x.com/2022/04/09/LPI9G6.png)|

## Usage

1. choose a folder to run your bot, like `mkdir some_bot && cd some_bot`
2. create a data dir in your bot folder, `mkdir data`
3. create a docker compose file, `wget https://raw.githubusercontent.com/naiba/twitter2telegram/main/docker-compose.yaml`
4. create a `.env` file, `wget -O .env https://raw.githubusercontent.com/naiba/twitter2telegram/main/.env.example`
5. update twitter/telegram tokens in `.env`
6. finally, run the bot `docker-compose up -d`
