#!/usr/bin/env bash
# Memory snapshot of a running Coppice desktop app, grouped by process kind.
#
#   scripts/mem-snapshot.sh [ELECTRON_PID]
#   VERBOSE=1 scripts/mem-snapshot.sh   # also list every process
#
# Walks the process tree under the Electron main process (found by name when
# no PID is given) and adds the bundled Postgres, which pg_ctl daemonizes out
# of that tree. Groups:
#   electron      Electron main, zygotes, GPU, renderers, utility processes
#   server        coppice-server
#   postgres      postmaster and its backends / background workers
#   connector     connector CLIs started by coppice-server (claude, codex, ...)
#   connector-sub everything those CLIs start (shells, tools, MCP servers)
#
# RSS counts shared pages in every process that maps them, so Postgres
# (shared_buffers) and Electron (shared libs) are overstated when summed.
# On Linux the PSS column splits shared pages fairly and is the better total.
set -euo pipefail

electron_pid="${1:-}"
if [[ -z "$electron_pid" ]]; then
  # Electron main: the `coppice` / `Coppice` process without a --type= flag.
  electron_pid=$(ps -A -o pid=,comm=,args= | awk '
    ($2 == "coppice" || $2 == "Coppice") && $0 !~ /--type=/ { print $1; exit }')
fi
if [[ -z "$electron_pid" ]] || ! kill -0 "$electron_pid" 2>/dev/null; then
  echo "Coppice desktop is not running (pass the Electron main PID)." >&2
  exit 1
fi

table=$(ps -A -o pid=,ppid=,rss=,comm=)
# A coppice-server PID works too (headless `coppice-server desktop` runs).
server_pid=$(echo "$table" | awk -v root="$electron_pid" '
  ($1 == root || $2 == root) && $4 == "coppice-server" { print $1; exit }')

postmaster_pid=""
if [[ -n "$server_pid" ]]; then
  data_dir=$(ps -o args= -p "$server_pid" | sed -n 's/.*--data-dir \([^ ]*\).*/\1/p')
  pid_file="${data_dir:-}/pg/data/postmaster.pid"
  if [[ -n "$data_dir" && -r "$pid_file" ]]; then
    postmaster_pid=$(head -n 1 "$pid_file")
  fi
fi

pss_kb() {
  local rollup="/proc/$1/smaps_rollup"
  if [[ -r "$rollup" ]]; then
    awk '/^Pss:/ { print $2; exit }' "$rollup" 2>/dev/null || echo 0
  else
    echo 0
  fi
}

# One line per process in the tree: pid group rss comm
classified=$(echo "$table" | awk \
  -v root="$electron_pid" -v server="${server_pid:-0}" -v postmaster="${postmaster_pid:-0}" '
  { parent[$1] = $2; rss[$1] = $3; comm[$1] = $4; order[++n] = $1 }
  function group(pid,   p) {
    if (pid == postmaster) return "postgres"
    if (pid == server) return "server"
    if (pid == root) return "electron"
    for (p = parent[pid]; p != "" && p != 0 && p != 1; p = parent[p]) {
      if (p == postmaster) return "postgres"
      if (p == server) return (parent[pid] == server ? "connector" : "connector-sub")
      if (p == root) return "electron"
    }
    return ""
  }
  END {
    for (i = 1; i <= n; i++) {
      pid = order[i]
      g = group(pid)
      if (g != "") print pid, g, rss[pid], comm[pid]
    }
  }')

if [[ -z "$classified" ]]; then
  echo "No processes found under PID $electron_pid." >&2
  exit 1
fi

with_pss=$(while read -r pid group rss comm; do
  echo "$pid $group $rss $(pss_kb "$pid") $comm"
done <<<"$classified")

echo "Coppice desktop memory (root PID $electron_pid, $(date '+%Y-%m-%d %H:%M:%S'))"
echo
echo "$with_pss" | awk '
  { rss[$2] += $3; pss[$2] += $4; count[$2]++; total_rss += $3; total_pss += $4; procs++ }
  END {
    printf "%-14s %6s %10s %10s\n", "group", "procs", "RSS MB", "PSS MB"
    split("electron server postgres connector connector-sub", groups, " ")
    for (i = 1; i <= 5; i++) {
      g = groups[i]
      printf "%-14s %6d %10.1f %10.1f\n", g, count[g], rss[g] / 1024, pss[g] / 1024
    }
    printf "%-14s %6d %10.1f %10.1f\n", "total", procs, total_rss / 1024, total_pss / 1024
  }'

if [[ "${VERBOSE:-0}" != "0" ]]; then
  echo
  printf "%-8s %-14s %10s %10s  %s\n" "pid" "group" "RSS MB" "PSS MB" "command"
  echo "$with_pss" | sort -k2,2 -k3,3nr | awk '
    { printf "%-8s %-14s %10.1f %10.1f  %s\n", $1, $2, $3 / 1024, $4 / 1024, $5 }'
fi
