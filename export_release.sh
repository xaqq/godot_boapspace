#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
godot_bin="${GODOT_BIN:-$HOME/.local/bin/godot4}"
godot_project="$repo_root/godot"
godot_log_dir="$godot_project/.godot/logs"
godot_config_home="${GODOT_XDG_CONFIG_HOME:-$godot_project/.godot/xdg_config}"

export_dir="${EXPORT_DIR:-$repo_root/export}"
export_preset="${GODOT_EXPORT_PRESET:-Linux}"
export_name="${EXPORT_NAME:-godot_boapspace.x86_64}"
export_path="${EXPORT_PATH:-$export_dir/$export_name}"

if [[ "$export_dir" != /* ]]; then
    export_dir="$repo_root/$export_dir"
fi

if [[ "$export_path" != /* ]]; then
    export_path="$repo_root/$export_path"
fi

cargo build --release --manifest-path "$repo_root/rust/Cargo.toml"

mkdir -p "$godot_log_dir" "$godot_config_home"
XDG_CONFIG_HOME="$godot_config_home" "$godot_bin" --headless --quiet --log-file "$godot_log_dir/import.log" --path "$godot_project" --import

mkdir -p "$(dirname -- "$export_path")"
XDG_CONFIG_HOME="$godot_config_home" "$godot_bin" --headless --quiet --log-file "$godot_log_dir/export_release.log" --path "$godot_project" --export-release "$export_preset" "$export_path"

printf 'Release exported to %s\n' "$export_path"
