# AGENTS.MD - rust/godot_bridge/src

This directory contains the Godot-facing Rust GDExtension classes for the project. Keep this layer thin: it should own Godot nodes, scene wiring, input, drawing, and signal dispatch, while durable game rules stay in `rust/game_engine`.

## Scope

- `lib.rs` registers the extension modules.
- `game_state.rs` owns the Bevy `World` and inserts ECS resources.
- `game_world.rs` is the main `Node2D`: it owns `GameState`, renders the grid, handles camera/input, exposes resource accessors, and emits gameplay signals.
- `resource_header.rs` and `tile_info_panel.rs` are HUD/panel controls backed by `.tscn` scenes.
- `root_menu.rs` and `ingame_menu.rs` are menu controls.

## Godot Rust Conventions

- Use typed Godot APIs and signal builders. Prefer `bind()` / `bind_mut()`, direct Rust method calls, and `signals().name().connect(...)` over `Gd::call("method_name", &[])`.
- Use `#[export]` fields with `OnEditor<Gd<T>>` for child node and cross-node references that are assigned in scenes.
- Check exported node references with `is_instance_valid()` before using them in lifecycle methods or per-frame code.
- Use `GString` or resource paths only where Godot APIs require them. Keep ordinary Rust logic on Rust types.
- Keep `Base<T>` as the final stored field and use `base()` / `base_mut()` for node operations.

## Scene Wiring

- When adding or renaming exported fields, update the matching scene file under `godot/`.
- `tile_info_panel.tscn` currently wires `pos_label` and `type_label`; its `game_world` reference is expected to be assigned by the containing scene.
- Panel classes should update existing labels or controls rather than finding nodes by string paths at runtime.

## Tile Selection

- Tile selection is intentionally owned by `GameWorld.selected_cell`, not by ECS.
- `GameWorld` emits `tile_selected(x, y, type_name)` and `tile_deselected()`.
- UI panels such as `TileInfoPanel` should listen to those signals and keep no independent selection source of truth.
- If tile data becomes game logic, add it to `game_engine::grid` first and expose it through typed `GameWorld` methods or signals.

## Resources

- `game_engine::resources::GameResources` is the source of truth for wood, stone, food, and gold.
- `GameWorld` exposes `#[func]` getters and mutators for Godot-facing code.
- Emit `resources_changed` after resource mutations. Existing HUD code may poll, but new event-driven UI should connect to the signal when practical.

## Build And Test

Run from the repository root:

```bash
cargo build --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
```

Open the Godot editor only when needed:

```bash
~/.local/bin/godot4 godot/project.godot --editor
```
