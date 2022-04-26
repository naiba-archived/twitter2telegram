FROM ubuntu:latest
ENV TZ="Asia/Shanghai"

RUN export DEBIAN_FRONTEND="noninteractive" && \
    apt update && apt install -y wget ca-certificates tzdata libsqlite3-dev perl gcc make && \
    update-ca-certificates && \
    ln -fs /usr/share/zoneinfo/$TZ /etc/localtime && \
    dpkg-reconfigure tzdata && \
    wget https://www.openssl.org/source/openssl-1.1.1f.tar.gz && \
    tar -xzvf openssl-1.1.1f.tar.gz && \
    cd openssl-1.1.1f && ./config && make install && mv *.so* /lib/x86_64-linux-gnu/ && \
    cd ../ && rm -rf openssl-1.1.1f*

WORKDIR /bot
COPY ./target/release/twitter2telegram ./bot
COPY ./migrations ./migrations

VOLUME ["/bot/data"]
CMD ["/bot/bot"]
