# syntax=docker/dockerfile:1.7
# Tendly self-host image: Rust server + production web build. No secrets are
# baked in; configure everything with environment variables at runtime.

FROM node:22-bookworm-slim AS web
WORKDIR /src
COPY package.json package-lock.json ./
COPY packages/contracts/package.json packages/contracts/
COPY apps/web/package.json apps/web/
COPY apps/native/package.json apps/native/
RUN npm ci --workspace @tendly/web --include-workspace-root=false --ignore-scripts
COPY packages/contracts packages/contracts
COPY apps/web apps/web
RUN npm run build -w @tendly/web

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY .cargo .cargo
COPY crates crates
RUN cargo build --release -p tendly-server --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /data tendly && mkdir -p /data && chown tendly /data
COPY --from=server /src/target/release/tendly /usr/local/bin/tendly
COPY --from=web /src/apps/web/dist /srv/tendly/web
COPY integrations/fixtures/mail /srv/tendly/fixtures/mail
USER tendly
ENV TENDLY_DATA_DIR=/data \
    TENDLY_WEB_DIR=/srv/tendly/web \
    TENDLY_FIXTURE_DIR=/srv/tendly/fixtures/mail \
    TENDLY_BIND=127.0.0.1:7878
VOLUME ["/data"]
EXPOSE 7878
HEALTHCHECK --interval=30s --timeout=3s CMD ["/bin/sh", "-c", "exec 3<>/dev/tcp/127.0.0.1/7878 && printf 'GET /healthz HTTP/1.0\\r\\nHost: localhost\\r\\n\\r\\n' >&3 && grep -q ok <&3"]
ENTRYPOINT ["tendly"]
CMD ["serve"]
