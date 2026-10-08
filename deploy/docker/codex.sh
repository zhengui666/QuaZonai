#!/usr/bin/env bash
# Published Codex image selection. Authentication/history stay in the native home.
# This file is also sourced by manage.sh while it holds the deployment lock.
if ! declare -F qz_get >/dev/null; then
  source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/manage.sh"
fi

qz_codex_exact_version() {
  local value=$1 part prerelease LC_ALL=C
  local pattern='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-([0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*))?$'
  # The application version parser includes a leading v in its 128-byte limit.
  [[ ${#value} -le 127 && $value =~ $pattern ]] ||
    qz_fail 'Codex requires an exact npm version, such as 0.157.0.'
  prerelease=${BASH_REMATCH[5]}
  while [[ -n $prerelease ]]; do
    part=${prerelease%%.*}
    [[ ! $part =~ ^0[0-9]+$ ]] || qz_fail 'Numeric prerelease identifiers cannot have leading zeroes.'
    if [[ $prerelease == *.* ]]; then prerelease=${prerelease#*.}; else break; fi
  done
  printf '%s\n' "$value"
}

qz_codex_requested_version() {
  if [[ $1 == latest ]]; then printf '%s\n' latest; else qz_codex_exact_version "$1"; fi
}

qz_codex_read_env() {
  local target
  # Parse data with awk, never source .env. NUL is rejected before Bash can drop it.
  target=$(LC_ALL=C awk '
    index($0,sprintf("%c",0)) { invalid=1 }
    /^[[:space:]]*($|#)/ { next }
    {
      if ($0 !~ /^[[:space:]]*CODEX_VERSION[[:space:]]*=[[:space:]]*[^[:space:]#]+[[:space:]]*(#.*)?$/) invalid=1
      line=$0; sub(/^[[:space:]]*CODEX_VERSION[[:space:]]*=[[:space:]]*/,"",line)
      sub(/[[:space:]#].*$/,"",line); value=line; count++
    }
    END {
      if (invalid || count != 1) {
        print ".env accepts exactly one CODEX_VERSION=<exact npm version> assignment and comments." > "/dev/stderr"
        exit 1
      }
      print value
    }' "$1") || return
  qz_codex_exact_version "$target"
}

qz_codex_docker() (
  local config=$1 socket
  shift
  socket=$(qz_get "$config" docker_socket) || return
  [[ $socket == /* && $socket != *$'\n'* && $socket != *$'\r'* ]] || qz_fail 'Codex requires a local Docker Unix socket.'
  unset DOCKER_CONTEXT
  export DOCKER_HOST="unix://$socket"
  exec docker "$@"
)

qz_codex_image_tag() {
  local value
  if value=$(qz_get "$1" codex_runtime_image 2>/dev/null) && [[ -n $value ]]; then
    printf '%s\n' "$value"
  else
    qz_get "$1" codex_image
  fi
}

qz_codex_image_id() {
  local config=$1 value tag
  tag=$(qz_codex_image_tag "$config") || return
  # Unlike inspect, this distinguishes an absent tag from a failed daemon query.
  value=$(qz_codex_docker "$config" image ls --quiet --no-trunc "$tag") || return
  [[ -z $value || $value =~ ^sha256:[0-9a-f]{64}$ ]] || qz_fail 'Codex image tag does not identify one local image.'
  printf '%s\n' "$value"
}

qz_codex_assert_lock() {
  local root
  root=$(qz_get "$1" root) || return
  [[ /proc/self/fd/9 -ef "$root/.deployment.lock" ]] || qz_fail 'Codex recovery requires the exclusive deployment lock.'
  flock --exclusive --nonblock 9 || qz_fail 'Codex recovery requires the exclusive deployment lock.'
}

qz_codex_require_stopped() {
  local config=$1 recover_created=${2:-false} containers container state tag
  tag=$(qz_codex_image_tag "$config") || return
  if [[ $recover_created == true ]]; then qz_codex_assert_lock "$config"; fi
  containers=$(qz_codex_docker "$config" container ls --all --quiet --no-trunc --filter "label=io.quazonai.codex.image=$tag") || return
  while IFS= read -r container; do
    [[ -n $container ]] || continue
    [[ $container =~ ^[0-9a-f]{12,64}$ ]] || qz_fail 'Docker returned an invalid Codex container identity.'
    state=$(qz_codex_docker "$config" inspect --format '{{.State.Status}}' "$container") || return
    case $state in
      created)
        if [[ $recover_created == true ]]; then
          # No force: Docker must still refuse an ID that started after inspect.
          qz_codex_docker "$config" container rm "$container" >/dev/null || return
        else
          qz_fail 'Codex sessions still exist. Finish or cancel/reconcile them before updating.'
        fi ;;
      exited|dead) ;;
      *) qz_fail 'Codex sessions still exist. Finish or cancel/reconcile them before updating.' ;;
    esac
  done <<< "$containers"
}

qz_codex_verify_candidate() {
  local config=$1 target=$2 candidate=$3 actual uid gid
  actual=$(qz_codex_docker "$config" run --rm --network none --read-only --cap-drop ALL \
    --security-opt no-new-privileges:true "$candidate" --version) || return
  [[ $actual == "codex-cli $target" ]] || qz_fail 'The pulled Codex executable does not match the requested version.'
  uid=$(qz_get "$config" uid) || return
  gid=$(qz_get "$config" gid) || return
  if ! qz_codex_docker "$config" run --rm --network none --read-only --cap-drop ALL \
    --security-opt no-new-privileges:true --security-opt seccomp=unconfined \
    --security-opt apparmor=unconfined --user "$uid:$gid" --pids-limit 128 \
    --memory 512m --memory-swap 512m \
    --tmpfs "/home/codex:rw,nosuid,nodev,mode=700,uid=$uid,gid=$gid,size=64m" \
    --tmpfs /tmp:rw,nosuid,nodev,size=64m --env CODEX_HOME=/home/codex \
    --entrypoint /usr/bin/timeout "$candidate" --signal=KILL 30 \
    /opt/codex/bin/codex --disable use_legacy_landlock -c 'sandbox_mode="read-only"' sandbox -- /usr/bin/true; then
    qz_fail "Candidate Codex sandbox failed; the installed image/version was not changed. On hosts restricting user namespaces with AppArmor, have an administrator install and load this bundle's codex.apparmor as described in README.md, then retry. Do not disable host-wide user-namespace policy or the Codex sandbox."
  fi
}

qz_codex_pull_candidate() {
  local config=$1 target=$2 reference=${3:-} selected metadata candidate platform version
  target=$(qz_codex_requested_version "$target") || return
  selected=${reference:-ghcr.io/zhengui666/quazonai-codex:$target}
  [[ -z $reference || $reference =~ ^(ghcr\.io/zhengui666/quazonai-codex@)?sha256:[0-9a-f]{64}$ ]] ||
    qz_fail 'A selected Codex image must be an immutable repository digest.'
  if [[ $selected != sha256:* ]]; then qz_codex_docker "$config" pull "$selected" || return; fi
  # One native formatted inspect freezes image identity and label together, with
  # no external JSON interpreter or mutable tag used for subsequent execution.
  metadata=$(qz_codex_docker "$config" image inspect --format '{{.Id}}|{{.Os}}|{{.Architecture}}|{{index .Config.Labels "org.opencontainers.image.version"}}' "$selected") || return
  candidate=${metadata%%|*}
  [[ $metadata == *'|'* ]] || qz_fail 'Docker did not report Codex image metadata.'
  metadata=${metadata#*|}; platform=${metadata%|*}; version=${metadata##*|}
  [[ $candidate =~ ^sha256:[0-9a-f]{64}$ && $platform == 'linux|amd64' ]] ||
    qz_fail 'The published Codex image is not a Linux x86_64 image.'
  version=$(qz_codex_exact_version "$version") || return
  [[ $target == latest || $target == "$version" ]] || qz_fail 'The published Codex image does not match the requested version.'
  qz_codex_verify_candidate "$config" "$version" "$candidate" || return
  QZ_CODEX_VERSION=$version
  QZ_CODEX_CANDIDATE=$candidate
}

qz_codex_switch() (
  set -euo pipefail
  local config=$1 target=$2 candidate=$3 root previous tag temporary started=false had_env=false
  root=$(qz_get "$config" root)
  qz_codex_assert_lock "$config"
  target=$(qz_codex_exact_version "$target")
  tag=$(qz_codex_image_tag "$config")
  previous=$(qz_codex_image_id "$config")
  temporary=$(mktemp -d "$root/.codex-switch.XXXXXXXX")
  qz_codex_switch_cleanup() {
    local result=$1 recovery_failed=false
    trap - EXIT INT TERM
    set +e
    if [[ $result != 0 && $started == true ]]; then
      if [[ -n $previous ]]; then
        qz_codex_docker "$config" image tag "$previous" "$tag" || recovery_failed=true
      else
        qz_codex_docker "$config" image rm "$tag" >/dev/null || recovery_failed=true
      fi
      if [[ $had_env == true ]]; then
        (qz_atomic_text "$root/.env" < "$temporary/previous.env") || recovery_failed=true
      else
        rm -f -- "$root/.env" || recovery_failed=true
        sync -f "$root" || recovery_failed=true
      fi
      if [[ $recovery_failed == true ]]; then
        printf '%s\n' 'Codex rollback was incomplete; reconcile the installed image and .env before continuing.' >&2
      fi
    fi
    rm -rf -- "$temporary"
    exit "$result"
  }
  trap 'qz_codex_switch_cleanup "$?"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  if [[ -e $root/.env ]]; then
    qz_codex_read_env "$root/.env" >/dev/null
    cp -- "$root/.env" "$temporary/previous.env"
    had_env=true
    LC_ALL=C awk -v version="$target" '
      /^[[:space:]]*CODEX_VERSION[[:space:]]*=/ { print "CODEX_VERSION=" version; next }
      { print }
    ' "$temporary/previous.env" > "$temporary/next.env"
  else
    printf 'CODEX_VERSION=%s\n' "$target" > "$temporary/next.env"
  fi
  started=true
  qz_codex_docker "$config" image tag "$candidate" "$tag"
  qz_atomic_text "$root/.env" < "$temporary/next.env"
)

qz_codex_prepare() {
  local config=$1 bundle=$2 root source target current installed reference='' version
  root=$(qz_get "$config" root)
  source=$root/.env
  if [[ ! -e $source ]]; then source=$bundle/.env; fi
  version=$(qz_get "$config" codex_version)
  if [[ -e $source ]]; then target=$(qz_codex_read_env "$source"); else target=$(qz_codex_exact_version "$version"); fi
  current=$(qz_codex_image_id "$config")
  if [[ -n $current ]]; then
    installed=$(qz_codex_docker "$config" image inspect --format '{{index .Config.Labels "org.opencontainers.image.version"}}' "$current")
    [[ $installed == "$target" ]] || qz_fail 'Codex image differs from .env; run codex-update.sh with the intended version before continuing.'
    if [[ ! -e $root/.env ]]; then printf 'CODEX_VERSION=%s\n' "$target" | qz_atomic_text "$root/.env"; fi
    return 0
  fi
  qz_codex_require_stopped "$config" true
  if [[ $version == "$target" ]]; then reference=$(qz_get "$config" codex_image); fi
  qz_codex_pull_candidate "$config" "$target" "$reference"
  qz_codex_switch "$config" "$QZ_CODEX_VERSION" "$QZ_CODEX_CANDIDATE"
}

qz_codex_update() {
  set -euo pipefail
  local root=$1 target=$2 reference=${3:-} config before runtime socket
  target=$(qz_codex_requested_version "$target")
  qz_configuration "$root"
  # Freeze only managed installation state, never the separate native home.
  # The protected temporary file is removed by the entrypoint's EXIT trap.
  config=$QZ_WORK/codex-configuration.json
  qz_raw "$QZ_CONFIG" '' > "$config"
  before=$(qz_raw "$config" '')
  runtime=$(qz_get "$config" codex_runtime_image 2>/dev/null) || qz_fail 'Upgrade the application to a prebuilt-image deployment bundle first.'
  socket=$(qz_get "$config" docker_socket 2>/dev/null) || qz_fail 'Upgrade the application to a prebuilt-image deployment bundle first.'
  [[ -n $runtime && -n $socket ]] || qz_fail 'Upgrade the application to a prebuilt-image deployment bundle first.'
  [[ ! -e $root/pending.json ]] || qz_fail 'Finish the pending application deployment before changing Codex.'
  qz_codex_pull_candidate "$config" "$target" "$reference"
  qz_lock "$root"
  qz_configuration "$root"
  [[ $(qz_raw "$QZ_CONFIG" '') == "$before" && ! -e $root/pending.json ]] ||
    qz_fail 'Application deployment changed during the Codex pull; retry.'
  qz_codex_read_env "$root/.env" >/dev/null
  qz_require_idle "$config"
  qz_codex_require_stopped "$config" true
  qz_codex_switch "$config" "$QZ_CODEX_VERSION" "$QZ_CODEX_CANDIDATE"
  printf 'Codex image updated to %s; native authentication and history were preserved.\n' "$QZ_CODEX_VERSION" || :
}

qz_codex_csv_field() {
  local value=$1
  if [[ $value == *','* || $value == *'"'* || $value == *$'\n'* || $value == *$'\r'* ]]; then
    value=${value//\"/\"\"}
    printf '"%s"' "$value"
  else
    printf '%s' "$value"
  fi
}

qz_codex_login() {
  set -euo pipefail
  local root=$1 status=${2:-false} config image name project uid gid home mount action tag socket login_pid='' login_result
  qz_lock "$root"
  qz_configuration "$root"
  config=$QZ_CONFIG
  [[ ! -e $root/pending.json ]] || qz_fail 'Finish the pending application deployment before logging in.'
  qz_require_idle "$config"
  qz_codex_require_stopped "$config" true
  image=$(qz_codex_image_id "$config")
  [[ -n $image ]] || qz_fail 'The installation has no Codex image; finish deployment or run codex-update.sh.'
  project=$(qz_get "$config" project)
  uid=$(qz_get "$config" uid); gid=$(qz_get "$config" gid); home=$(qz_get "$config" codex_home)
  name=$project-login-$(tr -d '-' < /proc/sys/kernel/random/uuid)
  tag=$(qz_codex_image_tag "$config")
  socket=$(qz_get "$config" docker_socket)
  mount="type=bind,$(qz_codex_csv_field "source=$home"),$(qz_codex_csv_field "target=$home")"
  qz_codex_login_cleanup() {
    local result=$1 remaining container
    trap - EXIT INT TERM
    set +e
    if [[ -n $login_pid ]]; then
      kill -TERM "$login_pid" 2>/dev/null
      wait "$login_pid" 2>/dev/null
    fi
    remaining=$(qz_codex_docker "$config" container ls --all --quiet --no-trunc --filter "name=^/$name$")
    while IFS= read -r container; do
      [[ $container =~ ^[0-9a-f]{12,64}$ ]] || continue
      qz_codex_docker "$config" container rm --force "$container" >/dev/null
    done <<< "$remaining"
    exit "$result"
  }
  trap 'qz_codex_login_cleanup "$?"' EXIT
  trap 'qz_codex_login_cleanup 130' INT
  trap 'qz_codex_login_cleanup 143' TERM
  action=--device-auth
  if [[ $status == true ]]; then action=status; fi
  # Native device URLs/codes go only to the operator terminal. No authentication
  # file is opened, copied, logged or placed in an image layer by this helper.
  (
    unset DOCKER_CONTEXT
    export DOCKER_HOST="unix://$socket"
    exec docker run --rm --name "$name" --label "io.quazonai.codex.image=$tag" \
    --user "$uid:$gid" --read-only --cap-drop ALL --security-opt no-new-privileges:true \
    --pids-limit 128 --memory 512m --memory-swap 512m --cpus 1 --init \
    --tmpfs "/home/codex:rw,nosuid,nodev,mode=700,uid=$uid,gid=$gid,size=16m" \
    --tmpfs /tmp:rw,nosuid,nodev,size=64m --mount "$mount" --env "CODEX_HOME=$home" \
    "$image" -c 'cli_auth_credentials_store="file"' login "$action"
  ) &
  login_pid=$!
  if wait "$login_pid"; then login_result=0; else login_result=$?; fi
  login_pid=''
  qz_codex_login_cleanup "$login_result"
}

qz_codex_main() {
  set -euo pipefail
  umask 077
  local root=${HOME:?}/.local/share/quazonai target=latest login=false status=false selected=false operation_pid='' operation_result
  while (($#)); do
    case $1 in
      --help|-h)
        printf '%s\n' 'Usage: codex-update.sh [VERSION|latest] [--directory PATH]' \
          '       codex-login.sh [--status] [--directory PATH]' \
          '' 'Pull a published Codex image or use native ChatGPT device-code login.' \
          'VERSION must be an exact published version, without a leading v.'
        return 0 ;;
      --directory)
        (($# >= 2)) || qz_fail '--directory requires a path.'
        root=$2; shift 2 ;;
      --directory=*) root=${1#*=}; shift ;;
      --login) login=true; shift ;;
      --status) status=true; shift ;;
      --*) qz_fail "Unknown option: $1" ;;
      *)
        [[ $selected == false ]] || qz_fail 'Select only one Codex version.'
        target=$1; selected=true; shift ;;
    esac
  done
  qz_require_bash
  [[ -n $root ]] || qz_fail '--directory requires a non-empty path.'
  case $root in '~') root=$HOME ;; '~/'*) root=$HOME/${root#\~/} ;; esac
  root=$(realpath -m -- "$root")
  QZ_WORK=$(mktemp -d "${TMPDIR:-/tmp}/quazonai-codex.XXXXXXXX")
  qz_codex_main_cleanup() {
    local result=$1
    trap - EXIT INT TERM
    if [[ -n ${operation_pid:-} ]]; then
      kill -TERM "$operation_pid" 2>/dev/null || :
      wait "$operation_pid" 2>/dev/null || :
    fi
    rm -rf -- "$QZ_WORK"
    exit "$result"
  }
  trap 'qz_codex_main_cleanup "$?"' EXIT
  trap 'qz_codex_main_cleanup 130' INT
  trap 'qz_codex_main_cleanup 143' TERM
  if [[ $login == true || $status == true ]]; then
    [[ $target == latest ]] || qz_fail 'Login/status does not select a version; use codex-update.sh first.'
    qz_codex_login "$root" "$status" &
  else
    qz_codex_update "$root" "$target" &
  fi
  operation_pid=$!
  if wait "$operation_pid"; then operation_result=0; else operation_result=$?; fi
  operation_pid=''
  qz_codex_main_cleanup "$operation_result"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then qz_codex_main "$@"; fi
