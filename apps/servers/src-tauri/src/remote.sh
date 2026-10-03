#!/usr/bin/env bash
# Host-side helpers for a Linux mxbserver, sent by MXB Servers over SSH on stdin:
#   ssh host 'bash -s -- <command> <observe port> [args]' < remote.sh
#
# There is no mxb-agent: the server is the `mxbserver` systemd unit (or a hand-started
# `bin/mxbserver`), and live control goes through mxbserver's own admin API, which the app
# reaches over an SSH tunnel with the token the user saved. This script only does what needs
# the host itself: reading and replacing the config file, systemctl, journalctl, track files.
#
# It works with both layouts:
#   systemd  the `mxbserver` unit: the arguments come from the unit's ExecStart (or
#            /etc/mxbserver/mxbserver.env), root-owned files are written with sudo
#            (passwordless on Lightsail's ubuntu).
#   bare     a hand-started `bin/mxbserver` (the deploy-<sha>.sh scripts): the process is the
#            one listening on the observe port, its arguments and directory are read from
#            /proc, and a restart is SIGINT + nohup, exactly as those scripts do.
#
# Commands (machine-readable lines start with "@@"):
#   read <obs>                    @@mode, @@config, @@sha, @@config_b64 <base64 of the file>
#   validate <obs> <b64>          the candidate run once for 1 s on side ports; @@valid 0|1
#   apply <obs> <b64> <sha>       back up, replace, restart, wait for /readyz; puts the backup
#                                 back and restarts again if the server isn't ready.
#                                 @@backup, @@result applied|rolled-back|failed
#   write <obs> <b64> <sha>       apply without a restart: check, back up and replace only; the
#                                 app then asks the running server to reload it (admin API).
#                                 @@backup, @@result written|failed
#   restore <obs> <backup>        put a backup from `write` back (the reload refused it).
#                                 @@result restored
#   admin-addr <obs>              @@admin <listen> from the config's [admin] listen (or the
#                                 --admin flag), else the default 127.0.0.1:9810
#   observe <obs>                 read /status and /readyz directly on the host
#   logs <obs> <lines>            journalctl -u mxbserver (the log file for a bare server)
#   service <obs> restart         systemctl restart mxbserver, wait for /readyz; @@result
#   tracks <obs>                  @@dir, @@package, @@paths_b64, @@tracks_b64: the .pkz files beside the
#                                 track package, in content/ and in content/tracks/
#   install-track <obs> <tmp> <name> <sha>      move an uploaded .pkz into the track folder
#   install-version <obs> <tmp> <name> <sha> <version>
#                                 replace the server binary, restart, roll back if not ready
set -uo pipefail

say() { printf '@@%s %s\n' "$1" "$2"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

UNIT=/etc/systemd/system/mxbserver.service
ENV_FILE=/etc/mxbserver/mxbserver.env
CMD="${1:-}"
OBS="${2:-}"
[[ "$OBS" =~ ^[0-9]{1,5}$ ]] || die "usage: <command> <observe port> [...]"

# The pid of our own mxbserver listening on 127.0.0.1:$OBS, if any.
listener() {
  ss -Hltnp "sport = :$OBS" 2>/dev/null | grep -o 'pid=[0-9]*' | head -n1 | cut -d= -f2
}

# Fill MODE, BIN, WD, ARGS (array), RUNAS, PID.
detect() {
  PID=""
  local unit_text
  unit_text="$(systemctl cat mxbserver 2>/dev/null)"
  if [[ -n "$unit_text" || -f "$UNIT" ]]; then
    MODE=systemd
    WD="$(systemctl show -p WorkingDirectory --value mxbserver 2>/dev/null)"
    [[ -n "$WD" ]] || WD=/opt/mxbserver
    local exec_line
    exec_line="$(sed -n 's#^ExecStart=[-@+!]*##p' <<< "$unit_text" | head -n1)"
    BIN="${exec_line%% *}"
    RUNAS="$(systemctl show -p User --value mxbserver 2>/dev/null)"
    [[ -n "$RUNAS" ]] || RUNAS=root
    local line word
    line="$(sudo -n sed -n 's/^MXBSERVER_ARGS=//p' "$ENV_FILE" 2>/dev/null | head -n1)"
    line="${line%\"}"; line="${line#\"}"
    read -r -a ARGS <<< "$line"
    if [[ ${#ARGS[@]} -eq 0 ]]; then
      # No env file: the arguments written straight into ExecStart.
      ARGS=()
      # shellcheck disable=SC2086 # the unit's own words, split on spaces
      for word in ${exec_line#"$BIN"}; do
        [[ "$word" == \$* ]] || ARGS+=("$word")
      done
    fi
    PID="$(systemctl show -p MainPID --value mxbserver 2>/dev/null)"
    [[ "$PID" == 0 ]] && PID=""
    return
  fi
  MODE=bare
  RUNAS="$(id -un)"
  PID="$(listener)"
  if [[ -n "$PID" && "$(cat "/proc/$PID/comm" 2>/dev/null)" != mxbserver ]]; then
    die "port $OBS is served by $(cat "/proc/$PID/comm" 2>/dev/null), not mxbserver"
  fi
  if [[ -z "$PID" ]]; then
    # Down: only safe to guess when exactly one of ours exists, or none (the deploy layout).
    local all
    all="$(pgrep -u "$RUNAS" -x mxbserver 2>/dev/null)"
    if [[ $(wc -w <<< "$all") -gt 1 ]]; then
      die "nothing answers on port $OBS and several mxbserver processes run; can't tell which is this server"
    fi
    PID="$all"
  fi
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
  # The deploy layout, when the arguments could not be read.
  if priv test -r "$WD/config/server.toml" 2>/dev/null; then echo "$WD/config/server.toml"; return; fi
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

sha_of() { priv sha256sum "$1" | cut -d' ' -f1; }

validate_file() { # run the candidate once on side ports, as the service would
  local cand="$1" args=() i skip=0 admin=0
  # Distinct side ports: the server refuses observe and admin on one port, and :0 twice
  # counts as the same.
  local side=$((20000 + RANDOM % 20000))
  for ((i = 0; i < ${#ARGS[@]}; i++)); do
    if [[ $skip == 1 ]]; then skip=0; continue; fi
    case "${ARGS[$i]}" in
      --config) args+=(--config "$cand"); skip=1 ;;
      # An admin listener from the command line moves off the live port too.
      --admin) args+=(--admin "127.0.0.1:$((side + 1))"); skip=1; admin=1 ;;
      *) args+=("${ARGS[$i]}") ;;
    esac
  done
  args+=(--listen 127.0.0.1:0 --observe "127.0.0.1:$side" --duration 1)
  [[ $admin == 0 ]] && has_admin "$cand" && args+=(--admin "127.0.0.1:$((side + 1))")
  local log rc
  log="$(mktemp)"
  ( cd "$WD" && as_owner timeout 60 "$BIN" "${args[@]}" ) > "$log" 2>&1
  rc=$?
  # A refusal is usually the first line, a crash the last: keep both ends.
  if (( $(wc -l < "$log") > 40 )); then head -n 6 "$log"; echo ...; tail -n 30 "$log"; else cat "$log"; fi
  rm -f "$log"
  return "$rc"
}

ready() { curl -fsS --max-time 1 "http://127.0.0.1:$OBS/readyz" >/dev/null 2>&1; }

wait_ready() { local i; for ((i = 0; i < 120; i++)); do ready && return 0; sleep 0.25; done; return 1; }

restart() {
  if [[ "$MODE" == systemd ]]; then
    # A crash-looping candidate can use up the unit's start limit; clear it first.
    sudo -n systemctl reset-failed mxbserver 2>/dev/null
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
  # `exec` makes the background subshell the server itself: no bash is left holding this
  # script's stdout (the SSH channel would never close) or standing in for the server's pid.
  # 9>&- keeps the apply lock out of the server, or no later apply could take it.
  ( cd "$WD" && exec setsid nohup "$BIN" "${ARGS[@]}" >> "$WD/logs/mxbserver.log" 2>&1 < /dev/null 9>&- ) &
  echo $! > "$WD/mxbserver.pid"
  sleep 0.5
  PID="$(cat "$WD/mxbserver.pid" 2>/dev/null)"
}


# toml_get <section> <key>: the quoted string value of `key` under `[section]`, or nothing.
toml_get() {
  priv cat "$CONFIG" 2>/dev/null | awk -v sec="$1" -v key="$2" '
    /^[[:space:]]*\[/ { s = $0; gsub(/[[:space:]]/, "", s); cur = s; next }
    cur == "[" sec "]" && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      sub(/^[^=]*=[[:space:]]*/, ""); sub(/[[:space:]]*(#.*)?$/, ""); gsub(/^"|"$/, ""); print; exit
    }'
}

# The configured [track] package, resolved the way mxbserver resolves it (args.rs `config_path`):
# a relative path is joined to the config file's own folder, an absolute one is used as is.
track_package() {
  local pkg
  pkg="$(toml_get track package)"
  [[ -n "$pkg" ]] || die "the config has no [track] package"
  [[ "$pkg" == /* ]] || pkg="$(dirname "$CONFIG")/$pkg"
  realpath -m "$pkg"
}

# The folder uploads go to: the configured package's folder. A package that points at a folder
# that does not exist (nothing to put a file beside) falls back to content/tracks when that exists.
track_dir() {
  local dir
  dir="$(dirname "$(track_package)")"
  if ! priv test -d "$dir" && priv test -d "$WD/content/tracks"; then dir="$WD/content/tracks"; fi
  echo "$dir"
}

# admin_listen: the admin API's address, from --admin or the config, else the default.
admin_listen() {
  local i listen
  for ((i = 0; i < ${#ARGS[@]}; i++)); do
    [[ "${ARGS[$i]}" == --admin ]] && { echo "${ARGS[$((i + 1))]:-}"; return; }
  done
  listen="$(toml_get admin listen)"
  echo "${listen:-127.0.0.1:9810}"
}

OWNER=""
if [[ "$CMD" =~ ^(read|validate|apply|write|restore|admin-addr|tracks|install-track|install-version|logs|service)$ ]]; then
  detect
  if [[ "$CMD" =~ ^(logs|service)$ ]]; then
    CONFIG=""
  else
    CONFIG="$(config_path)"
  fi
  [[ "$MODE" == systemd ]] && OWNER="$RUNAS"
fi

case "$CMD" in
  read)
    say mode "$MODE"; say config "$CONFIG"
    say sha "$(sha_of "$CONFIG")"
    say config_b64 "$(priv cat "$CONFIG" | base64 -w0)"
    ;;
  validate)
    cand="$(dirname "$CONFIG")/.candidate-$$.toml"
    decode "${3:?candidate}" "$cand"
    validate_file "$cand"; rc=$?
    priv rm -f "$cand"
    say valid "$([[ $rc == 0 ]] && echo 1 || echo 0)"
    ;;
  apply|write)
    b64="${3:?candidate}"; want="${4:?sha}"
    [[ "$want" =~ ^[0-9a-f]{64}$ ]] || die "bad sha"
    dir="$(dirname "$CONFIG")"
    # One apply at a time per config, from any client.
    exec 9> "/tmp/mxbserver-config-$(id -u).lock"
    flock -n 9 || die "another config change is being applied to this server"
    [[ "$(sha_of "$CONFIG")" == "$want" ]] || die "the config changed on the server since it was loaded; reload and try again"
    cand="$dir/.candidate-$$.toml"
    decode "$b64" "$cand"
    # `skipcheck`: the running server's own reload validates the file (and the old one is put
    # back if it refuses), so the slow run of the server binary is not needed.
    if [[ "${5:-}" != skipcheck ]] && ! validate_file "$cand"; then priv rm -f "$cand"; say result failed; die "the candidate did not pass the server's own check"; fi
    # Again, after the check's minute: nobody edited it meanwhile.
    if [[ "$(sha_of "$CONFIG")" != "$want" ]]; then priv rm -f "$cand"; die "the config changed on the server during the check; reload and try again"; fi
    stamp="$(date -u +%Y%m%dT%H%M%S.%NZ)-$$"
    backup="$dir/backups/$(basename "$CONFIG").$stamp"
    priv mkdir -p "$dir/backups"
    priv cp -p "$CONFIG" "$backup" || die "backup failed; nothing changed"
    # Keep the 20 newest backups.
    priv sh -c "ls -1t '$dir/backups/$(basename "$CONFIG")'.* 2>/dev/null | tail -n +21 | xargs -r rm -f"
    say backup "$backup"
    priv mv -f "$cand" "$CONFIG" || die "replace failed; the old config is still live"
    # `write`: the running server reloads the file itself; nobody is disconnected.
    if [[ "$CMD" == write ]]; then say result written; exit 0; fi
    if restart && wait_ready; then
      say result applied
    else
      echo "not ready after the change; putting the backup back" >&2
      priv cp -p "$backup" "$CONFIG"
      # Same arguments as before; only the process to stop may be new.
      if [[ "$MODE" == bare ]]; then
        PID="$(cat "$WD/mxbserver.pid" 2>/dev/null)"
        kill -0 "$PID" 2>/dev/null || PID=""
      fi
      if restart && wait_ready; then say result rolled-back; else say result failed; fi
    fi
    ;;
  restore)
    backup="${3:?backup}"
    dir="$(dirname "$CONFIG")"
    [[ "$backup" == "$dir/backups/$(basename "$CONFIG")."* && "$backup" != *..* ]] || die "not a backup of this config"
    priv test -f "$backup" || die "no such backup"
    priv cp -p "$backup" "$CONFIG" || die "restore failed"
    say result restored
    ;;
  admin-addr)
    say admin "$(admin_listen)"
    ;;
  observe)
    status="$(curl -fsS --max-time 4 "http://127.0.0.1:$OBS/status")" || die "nothing answers on observe port $OBS"
    say ready "$(curl -fsS --max-time 2 "http://127.0.0.1:$OBS/readyz" >/dev/null 2>&1 && echo 1 || echo 0)"
    say status_b64 "$(printf '%s' "$status" | base64 -w0)"
    ;;
  logs)
    n="${3:-200}"
    [[ "$n" =~ ^[0-9]{1,4}$ ]] || die "bad line count"
    if [[ "$MODE" == systemd ]]; then
      journalctl -u mxbserver -n "$n" --no-pager -o cat 2>/dev/null || sudo -n journalctl -u mxbserver -n "$n" --no-pager -o cat
    else
      tail -n "$n" "$WD/logs/mxbserver.log"
    fi
    ;;
  service)
    [[ "${3:-}" == restart ]] || die "unknown service action"
    if restart && wait_ready; then say result restarted; else say result failed; die "the server did not come back after the restart"; fi
    ;;
  tracks)
    pkg="$(track_package)"
    dir="$(dirname "$pkg")"
    say dir "$dir"
    say package "$pkg"
    # Every .pkz in the package's folder, plus content/ and content/tracks/ (where server
    # packages are kept), each folder once.
    paths=""
    seen=""
    for d in "$dir" "$WD/content" "$WD/content/tracks"; do
      d="$(realpath -m "$d")"
      [[ $'
'"$seen" == *$'
'"$d"$'
'* ]] && continue
      seen+="$d"$'
'
      paths+="$(priv find "$d" -maxdepth 1 -type f -iname '*.pkz' 2>/dev/null)"$'
'
    done
    paths="$(printf '%s' "$paths" | sed '/^$/d' | sort -u)"
    say paths_b64 "$(printf '%s' "$paths" | base64 -w0)"
    say tracks_b64 "$(printf '%s' "$paths" | sed 's#.*/##' | sort -u | base64 -w0)"
    ;;
  install-track)
    tmp="${3:-}"; name="${4:-}"; want="${5:-}"
    [[ "$tmp" =~ ^mxb-servers-[A-Za-z0-9_-]+$ ]] || die "bad temporary upload name"
    [[ "$name" =~ ^[A-Za-z0-9._-]+\.[Pp][Kk][Zz]$ && "$name" != .* ]] || die "bad track filename"
    [[ "$want" =~ ^[0-9a-f]{64}$ ]] || die "bad upload hash"
    file="/tmp/$tmp"
    [[ -f "$file" ]] || die "the uploaded file is missing"
    [[ "$(sha256sum "$file" | cut -d' ' -f1)" == "$want" ]] || die "the upload changed in transit"
    dir="$(track_dir)"
    owner=()
    [[ -n "$OWNER" ]] && owner=(-o "$OWNER")
    priv install -m 0644 "${owner[@]}" "$file" "$dir/$name" || die "cannot write $dir/$name"
    rm -f "$file"
    say installed "$name"
    ;;
  install-version)
    tmp="${3:-}"; want="${5:-}"; version="${6:-}"  # ${4:-} is the file name, unused here
    [[ "$tmp" =~ ^mxb-servers-[A-Za-z0-9_-]+$ ]] || die "bad temporary upload name"
    [[ "$want" =~ ^[0-9a-f]{64}$ ]] || die "bad upload hash"
    file="/tmp/$tmp"
    [[ -f "$file" ]] || die "the uploaded file is missing"
    [[ "$(sha256sum "$file" | cut -d' ' -f1)" == "$want" ]] || die "the upload changed in transit"
    [[ -n "$BIN" && -f "$BIN" ]] || die "cannot find the server binary to replace"
    old="$BIN.previous"
    priv cp -p "$BIN" "$old" || die "backup of the current binary failed; nothing changed"
    priv install -m 0755 "$file" "$BIN.new" || die "cannot write $BIN.new"
    rm -f "$file"
    priv mv -f "$BIN.new" "$BIN" || die "replace failed; the old binary is still live"
    if restart && wait_ready; then
      say result installed; say version "$version"
    else
      echo "not ready with the new binary; putting the old one back" >&2
      priv cp -p "$old" "$BIN"
      if restart && wait_ready; then say result rolled-back; else say result failed; fi
    fi
    ;;
  *) die "usage: read|validate|apply|admin-addr|observe|logs|service|tracks|install-track|install-version <observe port> [...]" ;;
esac
