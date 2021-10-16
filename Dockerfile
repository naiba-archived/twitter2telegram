FROM alpine:latest
ENV TZ="Asia/Shanghai"
RUN apk --no-cache --no-progress add \
    ca-certificates \
    sqlite-dev \
    libgcc \
    tzdata && \
    cp "/usr/share/zoneinfo/$TZ" /etc/localtime && \
    echo "$TZ" > /etc/timezone
run wget -q -O /etc/apk/keys/sgerrand.rsa.pub https://alpine-pkgs.sgerrand.com/sgerrand.rsa.pub && \
    wget https://github.com/sgerrand/alpine-pkg-glibc/releases/download/2.34-r0/glibc-2.34-r0.apk && \
    apk add glibc-2.34-r0.apk

WORKDIR /bot
COPY ./target/release/twitter2telegram ./bot

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
