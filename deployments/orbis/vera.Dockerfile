# syntax=docker/dockerfile:1

ARG VERA_REF

FROM golang:1.26.2 AS builder
ARG VERA_REF
RUN test -n "$VERA_REF" \
    && git init /src \
    && cd /src \
    && git remote add origin https://github.com/sourcenetwork/vera.git \
    && git fetch --depth 1 origin "$VERA_REF" \
    && git checkout --detach FETCH_HEAD
WORKDIR /src
ARG BUILD_JOBS=2
ENV GOMAXPROCS=${BUILD_JOBS} GOFLAGS="-p=${BUILD_JOBS}"
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    GODEBUG=http2client=0 go mod download \
    && GODEBUG=http2client=0 go build -o /out/verad ./cmd/verad

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates jq \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --home-dir /home/node node \
    && mkdir -p /home/node/.vera \
    && chown node:node /home/node/.vera
COPY --from=builder /out/verad /usr/local/bin/verad
USER node
