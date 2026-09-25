# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c AS package
ARG CODEX_VERSION
WORKDIR /build
RUN test -n "$CODEX_VERSION" \
 && npm install --ignore-scripts --no-audit --no-fund --package-lock=false "@openai/codex@$CODEX_VERSION" \
 && mkdir /bin-release \
 && cp node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/bin/* /bin-release/ \
 && cp node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/codex-path/* /bin-release/ \
 && cp -R node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/codex-resources /bin-release/codex-resources \
 && test -x /bin-release/codex-resources/bwrap \
 && /bin-release/codex --version

FROM debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git ripgrep tini coreutils libssl3 libstdc++6 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 1000 --create-home codex
COPY --from=package /bin-release/ /opt/codex/bin/
COPY --from=package /build/node_modules/@openai/codex/ /usr/share/doc/codex/
ARG CODEX_VERSION
LABEL org.opencontainers.image.title="QuaZonai Codex" \
      org.opencontainers.image.source="https://github.com/zhengui666/QuaZonai" \
      org.opencontainers.image.version=$CODEX_VERSION
ENV HOME=/home/codex CODEX_HOME=/home/codex/.codex \
    PATH=/opt/codex/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
USER 1000:1000
WORKDIR /home/codex
ENTRYPOINT ["/opt/codex/bin/codex"]
