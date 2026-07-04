# Tile Selection via Signals — Implementation Plan

**Goal:** Replace the shared `SelectedTile` Resource with signals emitted by `GameWorld` and received by interested nodes through `@export` references. This removes per-frame polling, clarifies ownership of selection state, and allows multiple components to each have their own selection signals in the future.

## Motivation

- `TileInfoPanel` currently polls the shared `SelectedTile` resource every frame, even when nothing changes.
- A single global `SelectedTile` resource makes it unclear who "owns" the selection. If the game later has multiple views or panels that can select tiles (e.g., map view, minimap, build overlay), a single shared resource becomes ambiguous.
- A signal + `@export` pattern is the Godot-idiomatic way for a child/sibling node to react to changes from another node.

## What Changes (conceptual)

### 1. `GameWorld` becomes the authoritative source for tile selection

- `GameWorld` emits a signal whenever a tile is selected or deselected.
- Internal selection state used for drawing the highlight rectangle is kept privately on `GameWorld` (no longer stored in a shared resource).

### 2. `TileInfoPanel` listens instead of polling

- `TileInfoPanel` gets a reference to `GameWorld` via an `@export` field (set in the scene via NodePath).
- On ready, it connects to `GameWorld`'s selection signals.
- Label updates happen only when a signal fires — no more `process()` polling.

### 3. Remove the `SelectedTile` resource

- The `SelectedTile` Rust struct (`rust/src/selected_tile.rs`) is deleted.
- The `selected_tile.tres` file (`godot/selected_tile.tres`) is deleted.
- The module registration in `lib.rs` is removed.

## Scene Wiring Changes (conceptual)

- `game_world.tscn` — remove the `selected_tile = ExtResource(...)` line from both the `GameWorld` and `TileInfoPanel` nodes, remove the `[ext_resource type="SelectedTile" ...]` line at the top. Add `game_world = NodePath("../GameWorld")` to `TileInfoPanel`'s node metadata.
- `tile_info_panel.tscn` — update `node_paths` metadata to include `game_world`.

## Files Affected

| File | Action |
|------|--------|
| `rust/src/game_world.rs` | Add signals, store selection internally, emit on click |
| `rust/src/tile_info_panel.rs` | Export `game_world` ref, connect to signals in ready, remove `process()` |
| `rust/src/selected_tile.rs` | **Delete** |
| `rust/src/lib.rs` | Remove `mod selected_tile` |
| `godot/selected_tile.tres` | **Delete** |
| `godot/game_world.tscn` | Remove SelectedTile ext_resource, rewire TileInfoPanel |
| `godot/tile_info_panel.tscn` | Add `game_world` to NodePath metadata (already has label paths; extend with new export field) |

## Build & Verify

```bash
cargo build --manifest-path rust/Cargo.toml
```

Open in Godot editor to confirm:
- Clicking a tile updates the TileInfoPanel labels instantly.
- Clicking the same tile again deselects (labels show "None"/"--" and highlight disappears).
- Clicking empty space deselects.
- The golden highlight rectangle still draws around the selected tile.
- No `SelectedTile` warnings or missing resource errors on scene load.
