#!/usr/bin/env bash
set -Eeuo pipefail
if [[ "${1:-console}" != console ]]; then
  exec /opt/quazonai/bin/server "$@"
fi
children=()
stop() {
  trap - TERM INT
  if ((${#children[@]})); then
    kill -TERM "${children[@]}" 2>/dev/null || true
    wait "${children[@]}" 2>/dev/null || true
  fi
}
trap 'stop; exit 143' TERM
trap 'stop; exit 130' INT
/opt/quazonai/bin/server serve &
children+=("$!")
XDG_CONFIG_HOME=/tmp/caddy-config XDG_DATA_HOME=/tmp/caddy-data \
  /usr/bin/caddy run --config /opt/quazonai/Caddyfile --adapter caddyfile &
children+=("$!")
set +e
wait -n "${children[@]}"
status=$?
set -e
stop
# An unexpected clean child exit must also restart the whole console.
((status != 0)) || status=1
exit "$status"
