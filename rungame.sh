#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
godot_bin="${GODOT_BIN:-$HOME/.local/bin/godot4}"
godot_project="$repo_root/godot"
godot_log_dir="$godot_project/.godot/logs"

cargo build --manifest-path "$repo_root/rust/Cargo.toml"

mkdir -p "$godot_log_dir"
"$godot_bin" --headless --log-file "$godot_log_dir/import.log" --path "$godot_project" --import

exec "$godot_bin" --path "$godot_project" "$@"
