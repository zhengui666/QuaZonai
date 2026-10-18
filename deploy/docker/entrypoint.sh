#!/usr/bin/env bash
set -euo pipefail
case "${1:-serve}" in
  serve|console)
    : "${QZ_CONFIG:?Persistent service config path is required}"
    : "${QZ_RESEARCH_PROVIDER_ENV:?Private provider environment file is required}"
    mkdir -p -- "$QZ_SCRATCH_DIR"
    # The service owns the only programmatic Pi Durable writer and scheduler.
    # tini forwards SIGTERM; the service drains HTTP and suspends the same DB.
    exec node --env-file="$QZ_RESEARCH_PROVIDER_ENV" --experimental-strip-types /opt/quazonai/apps/service/src/main.ts
    ;;
  cli) shift; exec node --experimental-strip-types /opt/quazonai/apps/cli/src/main.ts "$@" ;;
  configure) shift; exec node /opt/quazonai/deploy/docker/configure.mjs "$@" ;;
  maintenance) shift; exec node /opt/quazonai/deploy/docker/maintenance.mjs "$@" ;;
  job) shift; exec /opt/quazonai/bin/job "$@" ;;
  *) printf 'Use serve, cli, configure, maintenance or job.\n' >&2; exit 2 ;;
esac
