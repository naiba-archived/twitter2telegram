FROM ubuntu:16.04

ARG TARGETPLATFORM
ENV TZ="Asia/Shanghai"

RUN export DEBIAN_FRONTEND="noninteractive" && \
    apt update && apt install -y ca-certificates tzdata libssl1.0.0 && \
    update-ca-certificates && \
    ln -fs /usr/share/zoneinfo/$TZ /etc/localtime && \
    dpkg-reconfigure tzdata

WORKDIR /bot
COPY ./artifact/$TARGETPLATFORM/twitter2telegram ./bot
COPY ./migrations ./migrations

RUN chmod +x ./bot

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
