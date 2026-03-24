FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY agentim-server /usr/local/bin/agentim-server

EXPOSE 8900

ENTRYPOINT ["agentim-server"]
