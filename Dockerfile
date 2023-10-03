# This image uses cargo-chef to build the application in order to compile
# the dependencies apart from the main application. This allows the compiled
# dependencies to be cached in the Docker layer and greatly reduce the
# build time when there isn't any dependency changes.
#
# https://github.com/LukeMathWalker/cargo-chef
# https://github.com/RGB-WG/rgb-node/blob/master/Dockerfile

ARG SRC_DIR=/usr/local/src/satsbox
ARG BUILDER_DIR=/srv/satsbox

# build base
ARG BASE=base

# Base image
FROM rust:1.71.1-slim-bullseye as base

# mirror image for china
FROM base as mirror_cn

# Replace cn mirrors
ENV RUSTUP_DIST_SERVER=https://rsproxy.cn
RUN sed -i 's/deb.debian.org/mirrors.163.com/g' /etc/apt/sources.list
RUN echo '[source.crates-io]\nreplace-with = "mirror"\n[source.mirror]\nregistry = "https://rsproxy.cn/crates.io-index"' \
        >> $CARGO_HOME/config

FROM ${BASE} as chef

ARG SRC_DIR
ARG BUILDER_DIR

RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev protobuf-compiler

RUN rustup default stable
RUN rustup update
RUN cargo install cargo-chef --locked

WORKDIR $SRC_DIR

# Cargo chef step that analyzes the project to determine the minimum subset of
# files (Cargo.lock and Cargo.toml manifests) required to build it and cache
# dependencies
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

ARG SRC_DIR
ARG BUILDER_DIR

COPY --from=planner "${SRC_DIR}/recipe.json" recipe.json

# Build dependencies - this is the caching Docker layer
RUN cargo chef cook --release --recipe-path recipe.json --target-dir "${BUILDER_DIR}"

# Copy all files and build application # --all-features
COPY . .
RUN cargo build --release --target-dir "${BUILDER_DIR}" --bins

# build ui
FROM node:20-bullseye-slim as ui_base

# mirror image for china
FROM ui_base as ui_mirror_cn

# Replace cn mirrors
RUN sed -i 's/deb.debian.org/mirrors.163.com/g' /etc/apt/sources.list
RUN yarn config set registry 'https://registry.npm.taobao.org'

FROM ui_${BASE} as ui_builder
WORKDIR /app
COPY ui/package.json /app/package.json
RUN yarn install
COPY ui /app
RUN yarn build

# Final image with binaries
FROM debian:bullseye-slim as final

ARG SRC_DIR
ARG BUILDER_DIR

ARG BIN_DIR=/usr/local/bin
ARG HOME_DIR=/satsbox
ARG USER=satsbox

RUN adduser --home "${HOME_DIR}" --shell /bin/bash --disabled-login \
        --gecos "${USER} user" ${USER}
RUN mkdir ${HOME_DIR}/ui
RUN touch ${HOME_DIR}/satsbox.toml
RUN chown ${USER}:${USER} ${HOME_DIR}/ui ${HOME_DIR}/satsbox.toml

COPY --from=builder --chown=${USER}:${USER} \
        "${BUILDER_DIR}/release/satsbox" "${BIN_DIR}"
COPY --from=ui_builder --chown=${USER}:${USER} \
        "/app/dist" "${HOME_DIR}/ui/dist"

# Change default bind address to 0.0.0.0
ENV SATSBOX_NETWORK__HOST=0.0.0.0

WORKDIR "${HOME_DIR}"

USER ${USER}

EXPOSE 8080

ENTRYPOINT ["satsbox"]

CMD ["-c", "./satsbox.toml"]

