FROM rust:1.99.1-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release --bin pledge \
    && strip target/release/pledge

FROM node:24.22.2-bookworm-slim@sha256:01d566e8d94d8c619591d78652babfccd3c6992a3dffa9ed5ab39e11cf6c70e9

LABEL org.opencontainers.image.source="https://github.com/stentorian-io/pledge" \
      org.opencontainers.image.description="JavaScript and TypeScript package compatibility checks" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git openssh-client \
    && rm -rf /var/lib/apt/lists/* \
    && npm install --global --ignore-scripts --no-audit --no-fund npm@11.19.0 typescript@5.9.3 \
    && npm cache clean --force
COPY --from=build /build/target/release/pledge /usr/local/bin/pledge
COPY LICENSE-MIT LICENSE-APACHE /usr/share/licenses/pledge/
COPY scripts/action/package.json scripts/action/package-lock.json /usr/local/lib/pledge/
RUN npm ci --prefix /usr/local/lib/pledge --ignore-scripts --no-audit --no-fund \
    && npm cache clean --force
COPY scripts/action/*.cjs /usr/local/lib/pledge/
ENTRYPOINT ["pledge"]
