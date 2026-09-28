#!/usr/bin/env bash
# Config editing on a Linux mxbserver host, sent by MXB Servers over SSH on stdin:
#   ssh host 'bash -s -- <command> [args]' < remote.sh
#
# It works with both layouts:
#   systemd  /etc/systemd/system/mxbserver.service (deploy/opt/migrate-to-systemd.sh): the
#            arguments are in /etc/mxbserver/mxbserver.env, the service runs as `mxbserver`,
#            and root-owned files are written with sudo (passwordless on Lightsail's ubuntu).
#   bare     a hand-started `bin/mxbserver` (the deploy-<sha>.sh scripts): the arguments and
#            directory are read from the running process, and a restart is SIGINT + nohup,
#            exactly as those scripts do.
#
# Commands (machine-readable lines start with "@@"):
#   detect                           @@mode, @@config, @@bin, @@user, @@sha
#   read                             @@config_b64 <base64 of the config file>
#   validate <b64>                   the candidate run once for 1 s on side ports; @@valid 0|1
#   apply <b64> <sha> <observe port> back up, replace, restart, wait for /readyz; puts the
#                                    backup back and restarts again if the server isn't ready.
#                                    @@backup, @@result applied|rolled-back|failed
set -uo pipefail

say() { printf '@@%s %s\n' "$1" "$2"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

UNIT=/etc/systemd/system/mxbserver.service
ENV_FILE=/etc/mxbserver/mxbserver.env

# Fill MODE, BIN, WD, ARGS (array), RUNAS, PID.
detect() {
  PID=""
  if [[ -f "$UNIT" ]]; then
    MODE=systemd
    WD="$(systemctl show -p WorkingDirectory --value mxbserver 2>/dev/null)"
    [[ -n "$WD" ]] || WD=/opt/mxbserver
    BIN="$(sed -n 's#^ExecStart=\([^ ]*\).*#\1#p' "$UNIT" | head -n1)"
    RUNAS="$(systemctl show -p User --value mxbserver 2>/dev/null)"
    [[ -n "$RUNAS" ]] || RUNAS=root
    local line
    line="$(sudo -n sed -n 's/^MXBSERVER_ARGS=//p' "$ENV_FILE" 2>/dev/null | head -n1)"
    line="${line%\"}"; line="${line#\"}"
    read -r -a ARGS <<< "$line"
    PID="$(systemctl show -p MainPID --value mxbserver 2>/dev/null)"
    [[ "$PID" == 0 ]] && PID=""
    return
  fi
  MODE=bare
  RUNAS="$(id -un)"
  local p exe
  for p in $(pgrep -u "$RUNAS" -x mxbserver 2>/dev/null); do
    exe="$(readlink -f "/proc/$p/exe" 2>/dev/null)"; exe="${exe% (deleted)}"
    [[ "$exe" == "$HOME"/* ]] && { PID="$p"; break; }
  done
  if [[ -n "$PID" ]]; then
    BIN="$(readlink -f "/proc/$PID/exe")"; BIN="${BIN% (deleted)}"
    WD="$(readlink -f "/proc/$PID/cwd")"
    ARGS=()
    local first=1 a
    while IFS= read -r -d '' a; do
      if [[ $first == 1 ]]; then first=0; continue; fi
      ARGS+=("$a")
    done < "/proc/$PID/cmdline"
  else
    # Not running: the layout every deploy script has used.
    WD="$HOME/mxbserver"
    BIN="$WD/bin/mxbserver"
    ARGS=(--config config/server.toml --report 10)
  fi
}

config_path() {
  local i
  for ((i = 0; i < ${#ARGS[@]}; i++)); do
    if [[ "${ARGS[$i]}" == --config ]]; then
      local c="${ARGS[$((i + 1))]:-}"
      [[ "$c" == /* ]] && echo "$c" || echo "$WD/$c"
      return
    fi
  done
  die "the server's arguments have no --config"
}

as_owner() { # run a command as the service user (systemd) or ourselves (bare)
  if [[ "$MODE" == systemd && "$RUNAS" != "$(id -un)" ]]; then sudo -n -u "$RUNAS" "$@"; else "$@"; fi
}

priv() { # a command that may need root (systemd layout)
  if [[ "$MODE" == systemd ]]; then sudo -n "$@"; else "$@"; fi
}

decode() { # decode <b64> <dest>: the candidate, written as the file's owner can read it
  local tmp
  tmp="$(mktemp)"
  printf '%s' "$1" | base64 -d > "$tmp" || die "candidate is not base64"
  local owner=()
  [[ -n "$OWNER" ]] && owner=(-o "$OWNER")
  priv install -m 0644 "${owner[@]}" "$tmp" "$2" || die "cannot write $2"
  rm -f "$tmp"
}

has_admin() { # has_admin <file>: an [admin] section with a listen address
  awk '/^[[:space:]]*\[/{s=$0} s~/^[[:space:]]*\[admin\]/ && /^[[:space:]]*listen[[:space:]]*=/{f=1} END{exit !f}' "$1"
}

validate_file() { # run the candidate once on side ports, as the service would
  local cand="$1" args=() i skip=0
  for ((i = 0; i < ${#ARGS[@]}; i++)); do
    if [[ $skip == 1 ]]; then skip=0; continue; fi
    if [[ "${ARGS[$i]}" == --config ]]; then args+=(--config "$cand"); skip=1; continue; fi
    args+=("${ARGS[$i]}")
  done
  # Distinct side ports: the server refuses observe and admin on one port, and :0 twice
  # counts as the same.
  local side=$((20000 + RANDOM % 20000))
  args+=(--listen 127.0.0.1:0 --observe "127.0.0.1:$side" --duration 1)
  has_admin "$cand" && args+=(--admin "127.0.0.1:$((side + 1))")
  local log rc
  log="$(mktemp)"
  ( cd "$WD" && as_owner timeout 60 "$BIN" "${args[@]}" ) > "$log" 2>&1
  rc=$?
  # A refusal is usually the first line, a crash the last: keep both ends.
  if (( $(wc -l < "$log") > 40 )); then head -n 6 "$log"; echo ...; tail -n 30 "$log"; else cat "$log"; fi
  rm -f "$log"
  return "$rc"
}

ready() { curl -fsS --max-time 1 "http://127.0.0.1:$1/readyz" >/dev/null 2>&1; }

wait_ready() { local i; for ((i = 0; i < 120; i++)); do ready "$1" && return 0; sleep 0.25; done; return 1; }

restart() {
  if [[ "$MODE" == systemd ]]; then
    sudo -n systemctl restart mxbserver || return 1
    return 0
  fi
  if [[ -n "$PID" ]]; then
    kill -INT "$PID" 2>/dev/null
    local i
    for ((i = 0; i < 80; i++)); do kill -0 "$PID" 2>/dev/null || break; sleep 0.25; done
    kill -0 "$PID" 2>/dev/null && { echo "old server did not stop" >&2; return 1; }
  fi
  mkdir -p "$WD/logs"
  ( cd "$WD" && setsid nohup "$BIN" "${ARGS[@]}" >> "$WD/logs/mxbserver.log" 2>&1 < /dev/null & echo $! > "$WD/mxbserver.pid" )
  sleep 0.5
  PID="$(cat "$WD/mxbserver.pid" 2>/dev/null)"
}

detect
CONFIG="$(config_path)"
OWNER=""
[[ "$MODE" == systemd ]] && OWNER="$RUNAS"

case "${1:-}" in
  detect)
    say mode "$MODE"; say config "$CONFIG"; say bin "$BIN"; say user "$RUNAS"
    say sha "$(priv sha256sum "$CONFIG" | cut -d' ' -f1)"
    ;;
  read)
    say config_b64 "$(priv cat "$CONFIG" | base64 -w0)"
    say sha "$(priv sha256sum "$CONFIG" | cut -d' ' -f1)"
    ;;
  validate)
    cand="$(dirname "$CONFIG")/.candidate-$$.toml"
    decode "${2:?candidate}" "$cand"
    validate_file "$cand"; rc=$?
    priv rm -f "$cand"
    say valid "$([[ $rc == 0 ]] && echo 1 || echo 0)"
    ;;
  apply)
    b64="${2:?candidate}"; want="${3:?sha}"; obs="${4:?observe port}"
    [[ "$obs" =~ ^[0-9]+$ ]] || die "bad observe port"
    now="$(priv sha256sum "$CONFIG" | cut -d' ' -f1)"
    [[ "$now" == "$want" ]] || die "the config changed on the server since it was loaded; reload and try again"
    dir="$(dirname "$CONFIG")"
    cand="$dir/.candidate-$$.toml"
    decode "$b64" "$cand"
    if ! validate_file "$cand"; then priv rm -f "$cand"; say result failed; die "the candidate did not pass the server's own check"; fi
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    backup="$dir/backups/$(basename "$CONFIG").$stamp"
    priv mkdir -p "$dir/backups"
    priv cp -p "$CONFIG" "$backup" || die "backup failed; nothing changed"
    # Keep the 20 newest backups.
    priv sh -c "ls -1t '$dir/backups/$(basename "$CONFIG")'.* 2>/dev/null | tail -n +21 | xargs -r rm -f"
    say backup "$backup"
    priv mv -f "$cand" "$CONFIG" || die "replace failed; the old config is still live"
    if restart && wait_ready "$obs"; then
      say result applied
    else
      echo "not ready after the change; putting the backup back" >&2
      priv cp -p "$backup" "$CONFIG"
      # Same arguments as before; only the process to stop may be new.
      if [[ "$MODE" == bare ]]; then
        PID="$(cat "$WD/mxbserver.pid" 2>/dev/null)"
        kill -0 "$PID" 2>/dev/null || PID=""
      fi
      if restart && wait_ready "$obs"; then say result rolled-back; else say result failed; fi
    fi
    ;;
  *) die "usage: detect | read | validate <b64> | apply <b64> <sha> <observe port>" ;;
esac
