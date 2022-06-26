FROM ubuntu:latest
ARG TARGETPLATFORM
ENV TZ="Asia/Shanghai"

RUN export DEBIAN_FRONTEND="noninteractive" && \
    apt update && apt install -y wget ca-certificates tzdata && \
    update-ca-certificates libsqlite3-dev libssl-dev && \
    ln -fs /usr/share/zoneinfo/$TZ /etc/localtime && \
    dpkg-reconfigure tzdata

WORKDIR /bot
COPY ./artifact/$TARGETPLATFORM/twitter2telegram ./bot
COPY ./migrations ./migrations

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
