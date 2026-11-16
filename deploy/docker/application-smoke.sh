#!/usr/bin/env bash
# CI only: final application image, no host mounts, credentials, SSH or native owner.
set -euo pipefail
image=${1:?Application image required}
name="qz-application-smoke-${GITHUB_RUN_ID:-local}-${RANDOM}"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
# Check both static service imports and the dynamically reached supervisor module.
docker run --rm --network none --read-only --entrypoint node "$image" \
  --experimental-strip-types --input-type=module -e '
    await import("file:///opt/quazonai/apps/service/src/service.ts");
    await import("file:///opt/quazonai/apps/nautilus/src/remote-account-client.ts");
    await import("file:///opt/quazonai/apps/nautilus/src/remote-account-supervisor.ts");'
docker run --rm --network none --read-only "$image" cli --help >/dev/null
# Fresh tmpfs-only configuration has no account, dataset, target, credential or DB history.
docker run -d --name "$name" --network none --read-only \
  --tmpfs /tmp:rw,nosuid,nodev,mode=1777 --tmpfs /smoke:rw,nosuid,nodev,mode=1777 \
  -e HOME=/smoke/data/home -e QZ_CONFIG=/smoke/data/service.json \
  -e QZ_RESEARCH_PROVIDER_ENV=/smoke/data/research/provider.env \
  --entrypoint /usr/bin/tini "$image" -- /bin/bash -ec \
  '/opt/quazonai/entrypoint.sh configure /smoke http://127.0.0.1:8080; exec /opt/quazonai/entrypoint.sh serve' >/dev/null
for attempt in $(seq 1 60); do
  if docker exec "$name" curl -fsS --max-time 2 http://127.0.0.1:8080/health/live >/dev/null; then
    # The actual API must respond through its authentication layer, not only liveness.
    code=$(docker exec "$name" curl -sS -o /dev/null -w '%{http_code}' --max-time 2 http://127.0.0.1:8080/api/v3/accounts/configurations)
    test "$code" = 401
    docker stop --time 20 "$name" >/dev/null
    test "$(docker inspect -f '{{.State.ExitCode}}' "$name")" = 0
    exit 0
  fi
  if test "$(docker inspect -f '{{.State.Running}}' "$name")" != true; then break; fi
  sleep 2
done
docker logs --tail 120 "$name" >&2
exit 1
