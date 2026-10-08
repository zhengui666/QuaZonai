#!/usr/bin/env bash
# Boundary fixtures only: all Docker/model/install/network commands are stubs.
# This suite does not establish real Docker, sandbox, login or deployment results.
set -euo pipefail
umask 077
BUNDLE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/quazonai-codex-test.XXXXXXXX")
trap 'rm -rf -- "$TEST_ROOT"' EXIT
export TEST_FIXTURE=$TEST_ROOT/fixture CODEX_SCRIPT=$BUNDLE/codex.sh
mkdir -p "$TEST_ROOT/bin" "$TEST_FIXTURE" "$TEST_ROOT/home"
export PATH="$TEST_ROOT/bin:$PATH"
for binary in python python3 node jq npm npx curl wget sudo apt apt-get; do
  printf '#!/usr/bin/env bash\nprintf "Unexpected dependency: %%s\\n" %q >&2\nexit 99\n' "$binary" > "$TEST_ROOT/bin/$binary"
  chmod +x "$TEST_ROOT/bin/$binary"
done
cat > "$TEST_ROOT/bin/docker" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
{
  printf 'host=%q context=%q ' "${DOCKER_HOST:-}" "${DOCKER_CONTEXT:-}"
  printf '%q ' "$@"
  printf '\n'
} >> "$TEST_FIXTURE/docker.log"
if [[ $1 == pull ]]; then
  [[ ! -e $TEST_FIXTURE/pull-fails ]] || exit 21
  if [[ -e $TEST_FIXTURE/create-pending ]]; then printf '{}\n' > "$TEST_FIXTURE/install/pending.json"; fi
  if [[ -e $TEST_FIXTURE/change-config ]]; then
    sed 's/"port":8000/"port":8001/' "$TEST_FIXTURE/install/installation.json" > "$TEST_FIXTURE/changed.json"
    mv "$TEST_FIXTURE/changed.json" "$TEST_FIXTURE/install/installation.json"
  fi
  exit 0
fi
if [[ $1 == image && $2 == ls ]]; then
  [[ ! -e $TEST_FIXTURE/daemon-fails ]] || exit 22
  cat "$TEST_FIXTURE/image-id"
elif [[ $1 == image && $2 == inspect ]]; then
  [[ ! -e $TEST_FIXTURE/inspect-fails ]] || exit 23
  if [[ $4 == *'|'* ]]; then cat "$TEST_FIXTURE/metadata"; else cat "$TEST_FIXTURE/installed-version"; fi
elif [[ $1 == image && $2 == tag ]]; then
  [[ ! -e $TEST_FIXTURE/tag-fails ]] || { rm "$TEST_FIXTURE/tag-fails"; exit 24; }
  printf '%s\n' "$3" > "$TEST_FIXTURE/image-id"
elif [[ $1 == image && $2 == rm ]]; then
  : > "$TEST_FIXTURE/image-id"
elif [[ $1 == container && $2 == ls ]]; then
  if [[ ${!#} == name=* ]]; then
    if [[ -e $TEST_FIXTURE/login-id ]]; then cat "$TEST_FIXTURE/login-id"; fi
  else
    cat "$TEST_FIXTURE/containers"
  fi
elif [[ $1 == inspect ]]; then
  cat "$TEST_FIXTURE/container-state"
elif [[ $1 == container && $2 == rm ]]; then
  [[ ! -e $TEST_FIXTURE/remove-fails ]] || exit 25
  if [[ $3 == --force ]]; then rm -f "$TEST_FIXTURE/login-id"; fi
elif [[ $1 == run && ${!#} == --version ]]; then
  cat "$TEST_FIXTURE/binary-version"
elif [[ $1 == run && ${!#} == /usr/bin/true ]]; then
  [[ ! -e $TEST_FIXTURE/sandbox-fails ]] || exit 26
elif [[ $1 == run && ( ${!#} == --device-auth || ${!#} == status ) ]]; then
  printf '%064d\n' 1 > "$TEST_FIXTURE/login-id"
  if [[ -e $TEST_FIXTURE/login-waits ]]; then
    : > "$TEST_FIXTURE/login-started"
    trap 'exit 143' TERM
    while :; do sleep 0.05; done
  fi
  [[ ! -e $TEST_FIXTURE/login-fails ]] || exit 27
elif [[ $1 == compose ]]; then
  cat "$TEST_FIXTURE/active-runs"
else
  printf 'Unexpected Docker fixture call\n' >&2
  exit 98
fi
STUB
chmod +x "$TEST_ROOT/bin/docker"
OLD=sha256:$(printf 'a%.0s' {1..64})
NEW=sha256:$(printf 'b%.0s' {1..64})
export OLD NEW
COUNT=0
run() {
  bash -c 'set -euo pipefail; source "$CODEX_SCRIPT"; QZ_WORK=$TEST_FIXTURE/work; "$@"' bash "$@"
}
reset() {
  rm -rf "$TEST_FIXTURE"
  mkdir -p "$TEST_FIXTURE/install" "$TEST_FIXTURE/work" "$TEST_FIXTURE/native-home"
  printf '{"root":"%s","uid":%s,"gid":%s,"project":"quazonai-test","codex_home":"%s","codex_runtime_image":"quazonai-codex:test","codex_image":"ghcr.io/zhengui666/quazonai-codex@%s","codex_version":"0.157.0","docker_socket":"/fixture/docker.sock","bundle":"%s","image":"app-fixture","database_image":"database-fixture","password":"fixture-only","home":"/fixture/home","unit_directory":"/fixture/units","path":"/usr/bin:/bin","version":"v1.0.0","revision":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","port":8000,"database_port":5433}\n' \
    "$TEST_FIXTURE/install" "$(id -u)" "$(id -g)" "$TEST_FIXTURE/native-home" "$NEW" "$BUNDLE" > "$TEST_FIXTURE/install/installation.json"
  export CONFIG=$TEST_FIXTURE/install/installation.json ROOT=$TEST_FIXTURE/install
  printf '# retained comment\nCODEX_VERSION=0.156.1\n' > "$ROOT/.env"
  printf 'native-history-marker\n' > "$TEST_FIXTURE/native-home/history.txt"
  chmod 700 "$TEST_FIXTURE/native-home"
  printf '%s\n' "$OLD" > "$TEST_FIXTURE/image-id"
  printf '%s|linux|amd64|0.157.0\n' "$NEW" > "$TEST_FIXTURE/metadata"
  printf '0.156.1\n' > "$TEST_FIXTURE/installed-version"
  printf 'codex-cli 0.157.0\n' > "$TEST_FIXTURE/binary-version"
  printf '0\n' > "$TEST_FIXTURE/active-runs"
  printf 'exited\n' > "$TEST_FIXTURE/container-state"
  : > "$TEST_FIXTURE/containers"
  : > "$TEST_FIXTURE/docker.log"
}
pass() { COUNT=$((COUNT + 1)); printf 'ok %s - %s\n' "$COUNT" "$1"; }
fail() { printf 'not ok - %s\n' "$*" >&2; cat "$TEST_FIXTURE/output" >&2 2>/dev/null || :; exit 1; }
ok() { local title=$1; shift; "$@" > "$TEST_FIXTURE/output" 2>&1 || fail "$title"; pass "$title"; }
reject() { local title=$1; shift; if "$@" > "$TEST_FIXTURE/output" 2>&1; then fail "$title unexpectedly succeeded"; fi; pass "$title"; }
untouched() {
  [[ $(cat "$TEST_FIXTURE/image-id") == "$OLD" ]] || fail 'installed image changed after failure'
  grep -q '^CODEX_VERSION=0.156.1$' "$ROOT/.env" || fail 'selected version changed after failure'
  if grep -q 'image tag' "$TEST_FIXTURE/docker.log"; then fail 'candidate was activated on failed precheck'; fi
}
reset
for version in 0.157.0 1.0.0-rc.1 1.2.3-a-01 999999999999999999999999.2.3; do
  ok "exact version $version" run qz_codex_exact_version "$version"
done
for version in latest v0.157.0 '^0.157.0' '0.1' '0.1.2+meta' 01.2.3 1.2.3-01 '1.2.3-rc.01' 'file:/tmp/package' '1.x'; do
  reject "reject selector $version" run qz_codex_exact_version "$version"
done
ok 'latest remains an image selector' run qz_codex_requested_version latest
ok 'strict .env assignment' run qz_codex_read_env "$ROOT/.env"
for value in 'CODEX_VERSION=latest' 'CODEX_VERSION=$(touch attack)' 'DOCKER_HOST=remote' 'CODEX_VERSION=1.2.3 CODEX_VERSION=2.3.4'; do
  printf '%s\n' "$value" > "$ROOT/.env"
  reject 'reject non-version .env data' run qz_codex_read_env "$ROOT/.env"
done
printf 'CODEX_VERSION=1.2.3\nCODEX_VERSION=1.2.4\n' > "$ROOT/.env"
reject 'reject duplicate .env assignments' run qz_codex_read_env "$ROOT/.env"
printf 'CODEX_VERSION=1.2.\0003\n' > "$ROOT/.env"
reject 'reject NUL rather than silently dropping it' run qz_codex_read_env "$ROOT/.env"
reset
ok 'frozen latest image identity with sandbox check' run qz_codex_pull_candidate "$CONFIG" latest
grep -q "run .*${NEW} --version" "$TEST_FIXTURE/docker.log" || fail 'candidate version executed via mutable tag'
grep -q 'sandbox -- /usr/bin/true' "$TEST_FIXTURE/docker.log" || fail 'sandbox was not verified'
if grep -Eq -- '--privileged|--mount|--volume| build ' "$TEST_FIXTURE/docker.log"; then fail 'candidate verification changed privilege/mount/build boundary'; fi
reset
ok 'immutable local candidate skips pull' run qz_codex_pull_candidate "$CONFIG" 0.157.0 "$NEW"
! grep -q ' pull ' "$TEST_FIXTURE/docker.log" || fail 'local image was pulled'
reject 'reject mutable explicit reference' run qz_codex_pull_candidate "$CONFIG" 0.157.0 ghcr.io/zhengui666/quazonai-codex:latest
for mode in pull-fails inspect-fails sandbox-fails; do
  reset; touch "$TEST_FIXTURE/$mode"
  reject "$mode prevents activation" run qz_codex_update "$ROOT" latest
  untouched
done
reset; printf '%s|linux|arm64|0.157.0\n' "$NEW" > "$TEST_FIXTURE/metadata"
reject 'wrong architecture prevents activation' run qz_codex_update "$ROOT" latest
untouched
reset; printf '%s|linux|amd64|0.156.0\n' "$NEW" > "$TEST_FIXTURE/metadata"
reject 'wrong label version prevents activation' run qz_codex_update "$ROOT" 0.157.0
untouched
reset; printf 'codex-cli 0.156.0\n' > "$TEST_FIXTURE/binary-version"
reject 'wrong executable version prevents activation' run qz_codex_update "$ROOT" 0.157.0
untouched
reset; touch "$TEST_FIXTURE/daemon-fails"
reject 'daemon failure is not an absent tag' run qz_codex_image_id "$CONFIG"
reset; printf '{}\n' > "$ROOT/pending.json"
reject 'pending deployment refuses pull' run qz_codex_update "$ROOT" latest
[[ ! -s $TEST_FIXTURE/docker.log ]] || fail 'pending deployment contacted Docker'
for mode in create-pending change-config; do
  reset; touch "$TEST_FIXTURE/$mode"
  reject "$mode during pull refuses switch" run qz_codex_update "$ROOT" latest
  untouched
done
reset; printf '1\n' > "$TEST_FIXTURE/active-runs"
reject 'active runs refuse switch' run qz_codex_update "$ROOT" latest
untouched
for state in running paused restarting removing created; do
  reset; printf '%064d\n' 2 > "$TEST_FIXTURE/containers"; printf '%s\n' "$state" > "$TEST_FIXTURE/container-state"
  if [[ $state == created ]]; then
    reject 'created orphan recovery requires deployment lock' run qz_codex_require_stopped "$CONFIG" true
    reject 'created orphan is not removed without recovery' run qz_codex_require_stopped "$CONFIG"
  else
    reject "$state session prevents activation" run qz_codex_update "$ROOT" latest
    untouched
  fi
done
reset; printf '%064d\n' 2 > "$TEST_FIXTURE/containers"; printf 'created\n' > "$TEST_FIXTURE/container-state"
ok 'locked recovery removes never-started orphan' run bash -c 'source "$CODEX_SCRIPT"; qz_lock "$ROOT"; qz_codex_require_stopped "$CONFIG" true'
grep -q 'container rm 000' "$TEST_FIXTURE/docker.log" || fail 'created orphan not removed'
! grep -q 'rm --force' "$TEST_FIXTURE/docker.log" || fail 'orphan recovery forced running-capable removal'
reset
ok 'upgrade preserves independent Codex choice' run qz_codex_prepare "$CONFIG" "$BUNDLE"
! grep -q ' pull ' "$TEST_FIXTURE/docker.log" || fail 'independent Codex choice replaced during app upgrade'
grep -q '^CODEX_VERSION=0.156.1$' "$ROOT/.env" || fail 'independent version replaced'
reset; printf '0.155.0\n' > "$TEST_FIXTURE/installed-version"
reject 'tag/env disagreement refuses app prepare' run qz_codex_prepare "$CONFIG" "$BUNDLE"
reset; rm "$ROOT/.env"; : > "$TEST_FIXTURE/image-id"
ok 'new prepare uses pinned manifest and creates .env' run bash -c 'source "$CODEX_SCRIPT"; qz_lock "$ROOT"; qz_codex_prepare "$CONFIG" "$TEST_FIXTURE"'
grep -q "pull ghcr.io/zhengui666/quazonai-codex@$NEW" "$TEST_FIXTURE/docker.log" || fail 'initial image lost manifest pin'
grep -q '^CODEX_VERSION=0.157.0$' "$ROOT/.env" || fail 'new .env not written'
reset
DOCKER_HOST=tcp://unwanted DOCKER_CONTEXT=unwanted ok 'update uses saved Docker socket and clears context' run qz_codex_update "$ROOT" latest
grep -q 'host=unix:///fixture/docker.sock context=' "$TEST_FIXTURE/docker.log" || fail 'saved socket not used'
[[ $(cat "$TEST_FIXTURE/image-id") == "$NEW" ]] || fail 'new image not activated'
grep -q '^# retained comment$' "$ROOT/.env" || fail '.env comment lost'
[[ $(stat -c %a "$ROOT/.env") == 600 ]] || fail '.env permissions widened'
[[ $(stat -c %a "$TEST_FIXTURE/native-home") == 700 ]] || fail 'native home permissions changed'
[[ $(cat "$TEST_FIXTURE/native-home/history.txt") == native-history-marker ]] || fail 'native history changed'
reset
reject 'reported post-replace failure restores old image and exact .env' run bash -c '
  source "$CODEX_SCRIPT"; qz_lock "$ROOT"
  qz_atomic_text() {
    cat > "$1"
    if [[ ! -e $TEST_FIXTURE/failed-once ]]; then touch "$TEST_FIXTURE/failed-once"; return 41; fi
  }
  qz_codex_switch "$CONFIG" 0.157.0 "$NEW"
'
[[ $(cat "$TEST_FIXTURE/image-id") == "$OLD" ]] || fail 'rollback image differs'
[[ $(cat "$ROOT/.env") == $'# retained comment\nCODEX_VERSION=0.156.1' ]] || fail 'rollback text differs'
[[ -z $(find "$ROOT" -name '.codex-switch.*' -print) ]] || fail 'rollback scratch files leaked'
reset; touch "$TEST_FIXTURE/tag-fails"
reject 'failed tag switch restores old selection' run bash -c 'source "$CODEX_SCRIPT"; qz_lock "$ROOT"; qz_codex_switch "$CONFIG" 0.157.0 "$NEW"'
[[ $(cat "$TEST_FIXTURE/image-id") == "$OLD" ]] || fail 'failed tag rollback lost installed image'
reset; rm "$ROOT/.env"; : > "$TEST_FIXTURE/image-id"
reject 'first-install write failure removes candidate tag and .env' run bash -c '
  source "$CODEX_SCRIPT"; qz_lock "$ROOT"
  qz_atomic_text() { cat > "$1"; return 42; }
  qz_codex_switch "$CONFIG" 0.157.0 "$NEW"
'
[[ ! -e $ROOT/.env && ! -s $TEST_FIXTURE/image-id ]] || fail 'first-install failure left partial activation'
reset
(
  exec 9>>"$ROOT/.deployment.lock"
  flock -n 9
  if run qz_codex_update "$ROOT" latest > "$TEST_FIXTURE/output" 2>&1; then fail 'existing deployment lock allowed update'; fi
  untouched
)
pass 'exclusive deployment lock refuses update and stays owned by original process'
reset
ok 'native login command preserves directory and permissions' run qz_codex_login "$ROOT"
grep -q -- '--mount type=bind' "$TEST_FIXTURE/docker.log" || fail 'native auth home was not mounted'
grep -q -- 'login --device-auth' "$TEST_FIXTURE/docker.log" || fail 'native device auth not selected'
[[ ! -e $TEST_FIXTURE/login-id ]] || fail 'login container leaked'
[[ $(cat "$TEST_FIXTURE/native-home/history.txt") == native-history-marker ]] || fail 'native home changed'
reset; touch "$TEST_FIXTURE/login-fails"
reject 'failed login keeps failure and removes session container' run qz_codex_login "$ROOT"
[[ ! -e $TEST_FIXTURE/login-id ]] || fail 'failed login container leaked'
reset
ok 'native status selects status rather than new login' run qz_codex_login "$ROOT" true
grep -q ' login status' "$TEST_FIXTURE/docker.log" || fail 'status command changed'
reset; touch "$TEST_FIXTURE/login-waits"; mkdir "$TEST_FIXTURE/tmp"
TMPDIR="$TEST_FIXTURE/tmp" bash "$BUNDLE/codex-login.sh" --directory "$ROOT" > "$TEST_FIXTURE/output" 2>&1 &
login_process=$!
for attempt in {1..300}; do
  [[ -e $TEST_FIXTURE/login-started ]] && break
  sleep 0.01
done
if [[ ! -e $TEST_FIXTURE/login-started ]]; then
  kill -TERM "$login_process" 2>/dev/null || :
  wait "$login_process" || :
  fail 'login fixture never started'
fi
kill -TERM "$login_process"
set +e
wait "$login_process"
login_status=$?
set -e
[[ $login_status == 143 ]] || fail 'login signal exit status changed'
[[ ! -e $TEST_FIXTURE/login-id ]] || fail 'terminated login container leaked'
[[ -z $(find "$TEST_FIXTURE/tmp" -mindepth 1 -print) ]] || fail 'terminated login scratch state leaked'
pass 'TERM propagates to native login and cleans container and temporary state'
reset
ok 'CSV bind mount quotes comma and quote correctly' run bash -c 'source "$CODEX_SCRIPT"; [[ $(qz_codex_csv_field '\''source=/native/a,"b'\'') == '\''"source=/native/a,""b"'\'' ]]'
reset
ok 'update wrapper help without language runtime' bash "$BUNDLE/codex-update.sh" --help
ok 'login wrapper help without language runtime' bash "$BUNDLE/codex-login.sh" --help
reject 'login refuses version selection' bash "$BUNDLE/codex-login.sh" 0.157.0 --directory "$ROOT"
reject 'CLI rejects unknown option' bash "$BUNDLE/codex-update.sh" --invalid
reset; mkdir "$TEST_FIXTURE/tmp"
TMPDIR="$TEST_FIXTURE/tmp" ok 'standalone update wrapper completes without interpreter dependencies' bash "$BUNDLE/codex-update.sh" --directory "$ROOT" latest
[[ -z $(find "$TEST_FIXTURE/tmp" -mindepth 1 -print) ]] || fail 'completed update configuration snapshot leaked'
printf '1..%s\n' "$COUNT"
printf 'All checks used disposable boundary fixtures; no real Docker, login, download, installation or model execution occurred.\n'
