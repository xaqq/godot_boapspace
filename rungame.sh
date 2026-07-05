#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
godot_bin="${GODOT_BIN:-$HOME/.local/bin/godot4}"

cargo build --manifest-path "$repo_root/rust/Cargo.toml"

exec "$godot_bin" --path "$repo_root/godot" "$@"
