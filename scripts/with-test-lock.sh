#!/usr/bin/env bash
# Exclusive machine-wide lock for heavy Coppice Make test targets.
# Uses Python fcntl.flock (Linux + macOS). The Python parent holds the lock
# with CLOEXEC so children (embedded Postgres, rustc, …) do not inherit the
# fd and keep the lock after the suite exits.
set -euo pipefail

if [[ $# -lt 1 ]]; then
	echo "usage: $0 <command> [args...]" >&2
	exit 2
fi

CACHE_DIR="${HOME}/.cache/coppice"
LOCK_FILE="${CACHE_DIR}/test-run.lock"
mkdir -p "${CACHE_DIR}"

exec python3 - "${LOCK_FILE}" "$@" <<'PY'
import fcntl
import os
import signal
import subprocess
import sys

lock_path = sys.argv[1]
argv = sys.argv[2:]
fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o644)
try:
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    print(
        "Waiting for another Coppice test run to finish…",
        file=sys.stderr,
        flush=True,
    )
    fcntl.flock(fd, fcntl.LOCK_EX)

# Leave FD_CLOEXEC set so the lock fd is not inherited by test children.
proc = subprocess.Popen(argv)


def _forward(signum, _frame):
    if proc.poll() is None:
        proc.send_signal(signum)


signal.signal(signal.SIGINT, _forward)
signal.signal(signal.SIGTERM, _forward)
rc = proc.wait()
sys.exit(rc)
PY
