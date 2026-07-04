# AGENTS.md — godot_boapspace

Godot 4.7 game with a Rust GDExtension (`godot = "0.5"`) and a Bevy ECS game engine.

## Project vision

- 2D colony builder where each entity is simulated.
- The player gives high-level orders, and the simulation resolves the details.
- The game spans multiple surfaces / areas / planets that run in isolation.
- Tech stack: Godot 4.7 for UI and rendering, Rust for simulation.
- Simulation is written in Rust using Bevy ECS; Godot stays responsible for UI,
  rendering, input, and engine integration.

## Structure

```
rust/                       # Cargo workspace
  Cargo.toml                # Workspace root (members: game_engine, godot_bridge)
  game_engine/              # Pure Rust lib — Bevy ECS game logic (no Godot)
    Cargo.toml              # depends on bevy_ecs
    src/
      lib.rs
      grid.rs               # Grid resource (256x256 tile map)
      resources.rs          # GameResources resource (wood/stone/food/gold)
      systems.rs            # ECS schedule construction
      simulation.rs         # GameSimulation facade, owns independent surface Worlds
    tests/
      grid_tests.rs         # Integration tests for grid
      resource_tests.rs     # Integration tests for resources
  godot_bridge/             # GDExtension cdylib
    Cargo.toml              # depends on game_engine + godot
    src/
      lib.rs                # ExtensionLibrary entry point
      game_world.rs         # Main gameplay Node2D (owns GameSimulation, rendering, input)
      resource_header.rs    # HUD — polls GameWorld for resource values
      tile_info_panel.rs    # Selected tile info — listens to GameWorld signals
      root_menu.rs          # Main menu
      ingame_menu.rs        # Pause menu
godot/                      # Godot project (engine v4.7)
  project.godot
  godot_boapspace.gdextension
  main_ui.tscn
  game_world.tscn
  ingame_menu.tscn
  resource_header.tscn
  tile_info_panel.tscn
```

## Key design

- **Game logic (`game_engine`)**: `GameSimulation` owns independent surface runtimes. Each surface has its own Bevy `World`, `Grid`, `GameResources`, and `Schedule`. Pure Rust, no Godot dependency.
- **Godot bridge (`godot_bridge`)**: Owns one `GameSimulation` and calls `tick()` from Godot process code. Godot classes access the rendered surface through typed Rust methods.
- **Tile selection**: Stays entirely in Godot layer (`GameWorld.selected_cell`). Not in ECS.
- **Resources**: ECS `Resource<GameResources>` is the source of truth. `GameWorld` exposes `#[func]` getters/setters. `ResourceHeader` polls `GameWorld` each frame. The former `ResourceManager` autoload has been removed.
- **Signals**: `tile_selected`, `tile_deselected`, `resources_changed` all on `GameWorld`.

## Commands

```bash
cargo build --manifest-path rust/Cargo.toml           # Build workspace
cargo test --manifest-path rust/Cargo.toml            # Run all tests
~/.local/bin/godot4 godot/project.godot --editor      # Open editor
```

## Build & run

```bash
cargo build --manifest-path rust/Cargo.toml
~/.local/bin/godot4 godot/project.godot --editor
```

## Key facts

- `crate-type = ["cdylib"]` — builds a `.so` / `.dll` / `.dylib`, not an executable.
- Entry symbol: `gdext_rust_init` (godot-rust convention).
- Prefer strong types over strings: use typed method calls (`bind()`/`bind_mut()`, direct
  calls on `#[func]` methods) instead of `Gd::call("method_name", &[])`. Use `#[export]`
  fields of typed `Gd<T>` / `OnEditor<Gd<T>>` for child node references instead of
  `get_node("path")`. Only use `GString`/`StringName` where Godot APIs genuinely require
  strings (e.g. `change_scene_to_file`, resource paths).
- Tests in `game_engine/` — unit tests in `src/` via `#[cfg(test)]`, integration tests in `tests/`.
