#!/usr/bin/env bash
# Trims apt's download directory to the packages that are installed, at the
# version the file holds, before CI saves it as a cache. What it holds
# otherwise is whatever was left from earlier weeks, and downloads that never
# installed: a cache that only grows would soon cost more to restore than the
# mirror does to download from. Run it as root, after the last install.
set -euo pipefail

dir="${1:-/var/cache/apt/archives}"
kept=0
dropped=0

shopt -s nullglob
for deb in "$dir"/*.deb; do
  read -r name version < <(dpkg-deb --show --showformat='${Package} ${Version}\n' "$deb")
  # `ii` is installed and configured. `dpkg-query` prints a line for each
  # architecture a package is installed for, so ask for all of them.
  if dpkg-query --show --showformat='${db:Status-Abbrev} ${Version}\n' "$name" 2>/dev/null \
    | grep -qxF "ii  ${version}"; then
    kept=$((kept + 1))
  else
    rm -f -- "$deb"
    dropped=$((dropped + 1))
  fi
done

# `partial` is downloads in progress, and `lock` is only ever held by apt.
rm -rf -- "$dir/partial" "$dir/lock"

echo "apt cache: kept $kept installed packages, dropped $dropped"
