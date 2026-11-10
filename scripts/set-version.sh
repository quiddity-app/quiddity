#!/usr/bin/env bash
# Stamp a release version into every Cargo package, so that `quiddity-node
# --version` and the app's own metadata say what was released rather than the
# placeholder checked in. Usage: scripts/set-version.sh 1.2.3
#
# Only the workspace's version and the shell's are edited, and
# `cargo update --workspace` then rewrites just those versions in each
# Cargo.lock, which keeps the builds that follow valid under `--locked`.
# The checkout is a CI one, thrown away afterwards: don't run this on yours.
set -euo pipefail

version=${1:?usage: set-version.sh <semver>}
semver='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'
[[ $version =~ $semver ]] || { echo "not a semantic version: $version" >&2; exit 1; }

root=$(cd "$(dirname "$0")/.." && pwd)

# The first `version = ` in each file is the package's own, not a dependency's.
set_first_version() {
  local file=$1
  sed -i "0,/^version = \".*\"/s//version = \"$version\"/" "$file"
  grep -q "^version = \"$version\"" "$file" || { echo "no version found in $file" >&2; exit 1; }
}

set_first_version "$root/Cargo.toml"
set_first_version "$root/apps/app/src-tauri/Cargo.toml"

(cd "$root" && cargo update --workspace)
(cd "$root/apps/app/src-tauri" && cargo update --workspace)
