#!/bin/sh
# Create repos/ and config.toml inside a fresh private directory.
# Refuses symlinks and existing destinations. Does not use a shared /tmp path.
set -eu

die() {
  printf '%s\n' "$*" >&2
  exit 1
}

if [ "$#" -ne 2 ]; then
  die "usage: prepare-capture.sh CAPTURE_DIR EXAMPLE_TOML"
fi

dir=$1
src=$2
server_port=5000
web_port=5001

case "$dir" in
  /*) ;;
  *) die "capture directory must be absolute: $dir" ;;
esac

if [ -L "$dir" ]; then
  die "refusing symlink capture directory: $dir"
fi
if [ ! -d "$dir" ]; then
  die "capture directory does not exist: $dir"
fi

base=$(basename "$dir")
case "$base" in
  coppice-screenshot.*) ;;
  *) die "capture directory name must start with coppice-screenshot.: $base" ;;
esac

if [ -L "$src" ]; then
  die "refusing symlink config source: $src"
fi
if [ ! -f "$src" ]; then
  die "config source is not a regular file: $src"
fi

repos=$dir/repos
config=$dir/config.toml

if [ -L "$repos" ] || [ -e "$repos" ]; then
  die "refusing to reuse $repos"
fi
if [ -L "$config" ] || [ -e "$config" ]; then
  die "refusing to reuse $config"
fi

mkdir "$repos"
chmod 700 "$dir" "$repos"

set -C
cat "$src" > "$config"
set +C

tmp=$dir/config.toml.tmp
if [ -L "$tmp" ] || [ -e "$tmp" ]; then
  die "refusing to reuse $tmp"
fi

awk -v server="$server_port" -v web="$web_port" '
  /^desktop_allowed_hosts[[:space:]]*=/ { found = 1 }
  /^desktop_mode[[:space:]]*=[[:space:]]*true[[:space:]]*$/ && !inserted {
    print
    printf "desktop_allowed_hosts = [\"127.0.0.1:%s\", \"localhost:%s\", \"127.0.0.1\", \"localhost\", \"127.0.0.1:%s\", \"localhost:%s\"]\n", server, server, web, web
    inserted = 1
    next
  }
  { print }
  END {
    if (!inserted) exit 2
    if (found) exit 3
  }
' "$config" > "$tmp"
mv "$tmp" "$config"
chmod 600 "$config"

printf '%s\n' "$repos"
printf '%s\n' "$config"
