# AGENTS.md — godot_boapspace

Godot 4.7 game with a Rust GDExtension (`godot = "0.5"`).

## Structure

```
rust/              # Rust cdylib crate (godot_boapspace_rust)
  src/lib.rs       # ExtensionLibrary + Player class (Sprite2D)
  Cargo.toml
godot/             # Godot project (engine v4.7)
  project.godot
  godot_boapspace.gdextension
```

## Commands

- `cargo build` (from `rust/`) — compiles the GDExtension shared lib.
- Open `godot/project.godot` in the Godot 4.7 editor to run.
- The `.gdextension` file points lib paths to `res://../rust/target/`.

## Build & run

```bash
cargo build --manifest-path rust/Cargo.toml
~/.local/bin/godot4 godot/project.godot --editor
```

## Key facts

- `crate-type = ["cdylib"]` — builds a `.so` / `.dll` / `.dylib`, not an executable.
- Entry symbol: `gdext_rust_init` (godot-rust convention).
- `Player` struct extends `Sprite2D` via `#[derive(GodotClass)]` + `ISprite2D`.
- Prefer strong types over strings: use typed method calls (`bind()`/`bind_mut()`, direct
  calls on `#[func]` methods) instead of `Gd::call("method_name", &[])`. Use `#[export]`
  fields of typed `Gd<T>` / `OnEditor<Gd<T>>` for child node references instead of
  `get_node("path")`. Only use `GString`/`StringName` where Godot APIs genuinely require
  strings (e.g. `change_scene_to_file`, resource paths).
- No `.gitignore` exists yet. Before committing, add one ignoring `rust/target/`.
- No CI, no tests, no README.
