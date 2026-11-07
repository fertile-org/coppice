#!/bin/sh
# Remove one private capture directory created by mktemp.
# Refuses any other path, including a symlink.
set -eu

die() {
  printf '%s\n' "$*" >&2
  exit 1
}

if [ "$#" -ne 1 ]; then
  die "usage: remove-capture.sh CAPTURE_DIR"
fi

dir=$1

case "$dir" in
  /*) ;;
  *) die "refusing relative capture path: $dir" ;;
esac

if [ -L "$dir" ]; then
  die "refusing to remove symlink: $dir"
fi
if [ ! -d "$dir" ]; then
  die "not a capture directory: $dir"
fi

base=$(basename "$dir")
case "$base" in
  coppice-screenshot.*) ;;
  *) die "refusing to remove unexpected directory name: $base" ;;
esac

parent=$(dirname "$dir")
if [ -L "$parent" ]; then
  die "refusing to remove under a symlink parent: $parent"
fi

resolved=$(realpath "$dir")
if [ "$resolved" != "$dir" ]; then
  die "refusing capture path that resolves elsewhere: $dir -> $resolved"
fi

rm -rf -- "$dir"
