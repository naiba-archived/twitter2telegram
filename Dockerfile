FROM ubuntu:latest
ENV TZ="Asia/Shanghai"

RUN apt update && DEBIAN_FRONTEND="noninteractive" apt install -y ca-certificates tzdata \
    libsqlite3-dev && \
    update-ca-certificates && \
    ln -fs /usr/share/zoneinfo/$TZ /etc/localtime && \
    dpkg-reconfigure --frontend noninteractive tzdata

WORKDIR /bot
COPY ./target/release/twitter2telegram ./bot

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
