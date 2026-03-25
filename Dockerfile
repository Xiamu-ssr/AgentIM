FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY agentim-server /usr/local/bin/agentim-server

ENV AGENTIM_DATA_DIR=/data
VOLUME /data
EXPOSE 8900

ENTRYPOINT ["agentim-server"]
