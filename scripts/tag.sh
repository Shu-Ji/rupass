#!/bin/sh

set -eu

version="${1:-}"

if [ -z "$version" ]; then
  echo "usage: pnpm tag v1.0.0" >&2
  exit 1
fi

case "$version" in
  v*)
    ;;
  *)
    echo "tag must start with v, for example: v1.0.0" >&2
    exit 1
    ;;
esac

git rev-parse --git-dir >/dev/null 2>&1

if git rev-parse "$version" >/dev/null 2>&1; then
  echo "tag already exists: $version" >&2
  exit 1
fi

git tag "$version"
git push origin "$version"

echo "pushed tag: $version"
