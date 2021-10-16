FROM alpine:latest
ENV TZ="Asia/Shanghai"
RUN apk --no-cache --no-progress add \
    ca-certificates \
    sqlite-dev \
    libgcc \
    libc6-compat \
    tzdata && \
    cp "/usr/share/zoneinfo/$TZ" /etc/localtime && \
    echo "$TZ" > /etc/timezone
WORKDIR /bot
COPY ./target/release/twitter2telegram ./bot

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
