# godot_boapspace Planning Reference

Read this reference when planning feature or refactor work in this repository.

## Architecture

- `rust/game_engine` is the pure Rust simulation crate. It depends on `bevy_ecs` and must not depend on Godot.
- `rust/godot_bridge` is the GDExtension crate. It depends on `game_engine` and `godot`, owns a `GameSimulation`, and exposes typed Godot-facing methods.
- `godot` contains the Godot 4.7 project, scenes, and imported assets.
- `GameSimulation` owns independent surface runtimes. Each surface has its own Bevy `World`, `Grid`, `GameResources`, and `Schedule`.
- Godot owns rendering, input, camera behavior, UI, and selected tile UI state.
- ECS resources are the source of truth for simulation data such as `GameResources`.
- `GameWorld` signals include `tile_selected`, `tile_deselected`, and `resources_changed`.

## Planning Rules

- Prefer putting gameplay rules, world data, entity behavior, orders, jobs, resources, and multi-surface simulation state in `game_engine`.
- Prefer putting presentation, input handling, scene wiring, selection state, camera behavior, HUD polling, and Godot signals in `godot_bridge` or Godot scenes.
- Keep tile selection in Godot unless the requested feature needs selection to affect persistent simulation state.
- Keep each surface isolated unless the feature explicitly requires cross-surface coordination.
- Use typed Rust/Godot APIs where possible: exported typed node references, `bind()` / `bind_mut()`, and direct calls on `#[func]` methods. Avoid stringly `Gd::call` except where Godot APIs require strings.
- Treat `GameSimulation` as the facade between Godot bridge code and surface ECS internals.
- Avoid adding Godot dependencies to `game_engine`.
- Avoid reintroducing global resource manager state; resources live in ECS and are exposed through `GameWorld`.

## Validation Cues

- Use `cargo test --manifest-path rust/Cargo.toml` for Rust workspace tests.
- Use narrower tests in `rust/game_engine/tests` or unit tests when planning pure simulation changes.
- Use `cargo build --manifest-path rust/Cargo.toml` when the plan touches `godot_bridge`, GDExtension exports, or cross-crate APIs.
- Recommend Godot editor/runtime checks when scene wiring, exported node references, input, rendering, or signals are affected.

## Common Risk Patterns

- Putting simulation ownership in `GameWorld` because it is convenient to reach from Godot.
- Duplicating ECS resource state in UI nodes instead of polling or signaling from `GameWorld`.
- Creating APIs that accept strings where strong Rust types already exist.
- Planning a single-surface shortcut for a feature that must eventually work on isolated surfaces.
- Modifying scene files without checking the exported Rust fields and node references they need.
- Adding systems without defining their schedule placement, required resources/components, and tests.
