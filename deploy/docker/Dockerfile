# syntax=docker/dockerfile:1
FROM node:22.19.0-bookworm-slim@sha256:4a4884e8a44826194dff92ba316264f392056cbe243dcc9fd3551e71cea02b90 AS node

FROM node AS web
WORKDIR /build
COPY apps/web/package.json apps/web/package-lock.json ./apps/web/
RUN --mount=type=cache,target=/root/.npm npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
COPY apps/web ./apps/web
COPY packages/contracts ./packages/contracts
RUN npm --prefix apps/web run build

FROM node AS dependencies
WORKDIR /opt/quazonai
COPY apps/service/package.json apps/service/package-lock.json ./apps/service/
COPY apps/research/package.json apps/research/package-lock.json ./apps/research/
COPY apps/cli/package.json apps/cli/package-lock.json ./apps/cli/
# argon2's official package ships prebuilt platform binaries. No source bundler/SEA.
RUN --mount=type=cache,target=/root/.npm \
    npm ci --prefix apps/service --omit=dev --no-audit --no-fund \
 && npm ci --prefix apps/research --omit=dev --ignore-scripts --no-audit --no-fund \
 && npm ci --prefix apps/cli --omit=dev --ignore-scripts --no-audit --no-fund

FROM node AS application
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl bash tini libssl3 libstdc++6 python3 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/quazonai
COPY --from=dependencies /opt/quazonai/apps/ ./apps/
COPY apps/service/src ./apps/service/src
COPY apps/research/src ./apps/research/src
COPY apps/research/plugins ./apps/research/plugins
COPY apps/cli/src ./apps/cli/src
COPY packages/contracts ./packages/contracts
COPY contracts/generated ./contracts/generated
COPY --from=web /build/apps/web/dist/ ./web/
# Named Buildx context: the exact job and source-tools already tested by CI.
# No Rust compilation occurs in this Dockerfile.
COPY --from=native /job ./bin/job
COPY --from=native /source-tools ./operator/bin/source-tools
COPY --chmod=755 deploy/docker/operator/catalog-prepare deploy/docker/operator/polymarket-history ./operator/bin/
COPY runtimes/data/source_plugins.py runtimes/data/acquire.py runtimes/data/providers.py runtimes/data/snapshot.py runtimes/data/hf_dataset.py runtimes/data/polymarket_v2.py runtimes/data/binance_vision.py ./operator/
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/doc/quazonai/
COPY licenses/ /usr/share/doc/quazonai/licenses/
COPY --chmod=755 deploy/docker/entrypoint.sh ./entrypoint.sh
COPY deploy/docker/configure.mjs deploy/docker/maintenance.mjs ./deploy/docker/
ARG VERSION=development
ARG REVISION=unknown
LABEL org.opencontainers.image.title="QuaZonai" \
      org.opencontainers.image.source="https://github.com/zhengui666/QuaZonai" \
      org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
ENV QZ_JOB_BINARY=/opt/quazonai/bin/job QZ_SCRATCH_DIR=/tmp TMPDIR=/tmp
USER node
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=6 \
 CMD curl -fsS --max-time 4 http://127.0.0.1:8080/health/live >/dev/null
ENTRYPOINT ["/usr/bin/tini", "--", "/opt/quazonai/entrypoint.sh"]
CMD ["serve"]
