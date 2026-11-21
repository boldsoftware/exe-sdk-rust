#!/bin/sh
# Copies the generated SDK from an exe.dev repository checkout, and checks
# that this crate's dependencies are the ones it was built and tested with.
# Usage: scripts/sync.sh [path/to/exe]   (default: ../exe)
set -eu
exe=${1:-../exe}
rm -rf src
cp -R "$exe/sdk/rust/src" src
cp "$exe/sdk/rust/rustfmt.toml" rustfmt.toml
# The [dependencies] and [features] tables must match the generated Cargo.toml.
tables() { sed -n '/^\[\(dependencies\|features\)\]/,/^\[/p' "$1" | grep -v '^\[dev-\|^#' ; }
if ! tables "$exe/sdk/rust/Cargo.toml" | diff -u - "$(tables Cargo.toml >/tmp/exedev-sync.$$ && echo /tmp/exedev-sync.$$)"; then
	rm -f /tmp/exedev-sync.$$
	echo "sync.sh: Cargo.toml's dependencies differ from $exe/sdk/rust/Cargo.toml; update them" >&2
	exit 1
fi
rm -f /tmp/exedev-sync.$$
