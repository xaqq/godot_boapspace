# Construction Footer Implementation Plan

## Goal and current context

Replace the vertically stacked building palette and Roads panel in the left
sidebar with a collapsible construction dock at the bottom of the game area.
The dock must make construction easier to browse, show useful card and
placement feedback, and preserve all existing construction rules.

The current UI is split between `building_palette.rs` / `building_palette.tscn`
and `roads_panel.rs` / `roads_panel.tscn`. Both are instantiated in the
left-side `ScrollContainer` in `game_world.tscn`. `GameWorld` owns the
authoritative transient placement mode and already supports repeated building
placement, persistent road strokes, cancellation, surface-switch cancellation,
and live validation.

## Decided user-facing behavior

- Keep an always-visible compact construction footer inside the game area.
- `B` and a `Build [B]` button expand or collapse a drawer above the footer.
- The drawer starts collapsed and remembers the selected category during the
  lifetime of the game view; this UI state is not saved.
- Collapsing the drawer does not cancel placement.
- The compact footer shows the active tool, cancellation hint, and a Cancel
  button while placement is active.
- Escape, right-click over the map, and Cancel end placement. Successful
  building and road placement remains repeat-until-cancel.
- Categories, in order, are Storage, Production, Processing, Housing, Civic,
  and Roads.
- Field and Tree Plot remain contextual actions on selected Farm and Forester's
  Lodge buildings and do not appear in the global dock.
- Cards show existing artwork, name, footprint or per-tile scope, cost, and a
  description. Current resource stock never disables a card.
- Building placement exposes a live valid/invalid reason. Roads retain active
  tier, stroke cell count, aggregate cost, and validation feedback.
- Surface changes cancel the active tool but preserve drawer/category UI state.
- Full-screen panels take visual/input precedence and do not implicitly cancel
  construction placement.

## Categories and card content

- Storage: Depot, Warehouse.
- Production: Farm, Forester's Lodge.
- Processing: Sawmill, Stoneworks, Kitchen.
- Housing: Small House, Medium House, Large House.
- Civic: Town Hall.
- Roads: Dirt Path, Cobblestone Road, Flagstone Road.

Descriptions must accurately reflect current simulation behavior: storage
capacities, Farm/Field and Lodge/Tree Plot relationships, refinery recipes,
housing capacities, Town Hall as a civic landmark without claiming a new
effect, and road movement multipliers.

## Non-goals

- No construction, resource, labor, terrain, collision, or road-rule changes.
- No unlock system, affordability gate, demolition, search, favorites,
  per-building hotkeys, controller navigation, or new artwork.
- No redesign of surface, tile, NPC, or selected-building information.
- No change to contextual Field or Tree Plot placement.

## Relevant files and existing patterns

- `godot/world/game_world.tscn`: main layout, left sidebar, game overlays.
- `godot/panel/building_palette.tscn` and
  `rust/godot_bridge/src/panel/building_palette.rs`: current typed building
  button wiring and palette-order test.
- `godot/panel/roads_panel.tscn` and
  `rust/godot_bridge/src/panel/roads_panel.rs`: road selection and live status.
- `rust/godot_bridge/src/world/game_world.rs`: authoritative placement mode,
  previews, input handling, placement commands, surface switching, and asset
  mappings.
- `rust/godot_bridge/src/assets.rs`: typed `ResourceLoader` helpers.
- `rust/game_engine/src/buildings.rs`: building labels, footprints, costs,
  capacities, and validation.
- `rust/game_engine/src/roads.rs`: road labels, costs, speeds, and validation.
- `godot/project.godot`: input action definitions.
- Toggle-panel and runtime-card patterns in `task_list_panel.rs`,
  `performance_info.rs`, and `resource_panel.rs`.

## Contract and UI changes

1. Add a dedicated `construction_toggle` input action bound to physical `B`.
2. Add a Rust-backed construction dock scene under `godot/panel/` and a
   matching bridge module.
3. Represent the six categories and fourteen global tools as bridge/UI catalog
   metadata. Read authoritative costs, footprints, housing capacities, and road
   properties from `game_engine` definitions.
4. Reuse imported building and road textures through `ResourceLoader`; expose
   shared asset-path mappings from the bridge asset module rather than
   duplicating authoritative paths.
5. Expose a typed, read-only placement status from `GameWorld` that covers the
   active tool plus building validation or road stroke information. The dock
   must refresh from this authoritative state so cancellation and surface
   changes cannot leave stale highlights.
6. Expose typed generic bridge commands for selecting a building or road tool
   and cancelling placement. Contextual plot tools remain represented in status
   but never become global cards.
7. Show live building validation messages and compact road summaries in the
   footer; retain detailed road errors in tooltip text when several cells fail.
8. Instantiate the dock as a game-area overlay below full-screen modal panels,
   and remove the old palette and Roads instances from the left sidebar.
9. Retire the obsolete palette and Roads panel modules/scenes after their
   behavior and relevant tests have moved to the dock.

## Implementation sequence

1. Centralize shared construction texture-path lookup in the bridge assets
   module and preserve the existing resource-loading behavior.
2. Add the construction catalog, descriptions, formatting helpers, and unit
   tests for exhaustive membership, ordering, copy, costs, and exclusions.
3. Add typed placement-state and validation presentation queries to
   `GameWorld`, plus generic typed start/cancel entry points used by the dock.
4. Build the construction dock Rust class: category switching, dynamic cards,
   toggle/cancel input, authoritative active styling, and status refresh.
5. Add the dock scene, input action, and main game scene integration.
6. Remove obsolete palette/Roads scene integration and modules.
7. Format, run focused tests, run the Rust workspace tests, and load the Godot
   project headlessly to catch registration or scene-wiring failures.

## Risks and edge cases

- UI-local button state can drift after Escape, right-click, surface changes,
  or contextual placement; authoritative `GameWorld` status avoids this.
- The overlay must intercept mouse input without blocking the map outside its
  visible footer/drawer controls.
- Road atlas assets may need a presentation crop or intentional scaled use as
  card artwork.
- Long road validation output must stay compact while preserving access to all
  details.
- Building placement currently logs failures but does not expose a label; the
  bridge presentation mapping must cover every `BuildingPlacementError`.
- Imported image files must continue through Godot's `ResourceLoader` pipeline.
- Full-screen panels must render above the construction dock.

## Tests and validation

- Unit tests for category order and exhaustive global tool membership.
- Explicit exclusion tests for Field and Tree Plot.
- Unit tests for non-empty descriptions and player-facing labels.
- Formatting tests for building costs, road aggregate cost, and validation
  messages.
- State tests for collapsed default, category changes, placement preservation
  on collapse, active selection, cancellation, and surface-switch clearing.
- Scene-content or headless-load checks for typed exported node references.
- `cargo fmt --manifest-path rust/Cargo.toml --all --check`
- `cargo test --manifest-path rust/Cargo.toml`
- `~/.local/bin/godot4 --headless --path godot --quit-after 2
  res://world/game_world.tscn`

## Acceptance criteria

- The left sidebar contains no global building or road construction controls.
- The compact footer is always present and the drawer toggles through `B` and
  Build without cancelling placement.
- All eleven global buildings and all three road tiers appear exactly once in
  the agreed category order; contextual plots are absent.
- Every card shows the agreed facts and starts the correct typed placement
  tool.
- Active highlighting and footer status clear correctly after every external
  cancellation path.
- Building invalid reasons and road stroke feedback are player-visible.
- Existing repeated placement, surface isolation, contextual plots, and
  resource-waiting behavior remain unchanged.
- No click passes through the visible dock to place on the map.
- Rust tests pass and the Godot scene loads headlessly without registration or
  exported-reference errors.

## Open questions

None.
