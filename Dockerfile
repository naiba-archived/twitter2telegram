FROM ubuntu:latest
ENV TZ="Asia/Shanghai"

RUN export DEBIAN_FRONTEND="noninteractive" && apt update && apt install -y ca-certificates tzdata \
    libsqlite3-dev && \
    update-ca-certificates && \
    ln -fs /usr/share/zoneinfo/$TZ /etc/localtime && \
    dpkg-reconfigure tzdata

WORKDIR /bot
COPY ./target/release/twitter2telegram ./bot
COPY ./migrations ./migrations

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
