# Collisions Feature

## Summary And Goals

Add an engine-owned, grid-tile collision model to support stronger building
placement validation and prepare for future pathfinding. Collisions are
tile-level only, not pixel-perfect.

The v1 feature focuses on placement rules and queryable collision data. Current
NPC movement behavior does not change yet.

## Non-Goals

- No NPC-on-NPC collision.
- No pathfinding implementation.
- No Godot physics/navigation collision.
- No player-facing invalid-placement reason text beyond existing green/red
  previews.
- No relocation/despawn retrofit for entities already on blocked tiles.

## User-Facing Behavior

- Building and field placement previews stay green/red.
- Invalid placement rejects the command.
- Resource nodes block building placement.
- NPCs do not block building placement.
- Initial NPC spawn positions remain unchanged, even if future collision queries
  would mark a tile blocked.

## Simulation Rules

- Collision state is owned by `game_engine` and scoped per surface.
- Use layered collision flags, not one global "solid" value:
  - Build blocking.
  - NPC walk blocking.
- Terrain placement:
  - Warehouse, TownHall, and Farm: Grass, Dirt, or Sand.
  - Field: Grass or Dirt.
  - Water: never buildable.
- Terrain walkability:
  - Grass, Dirt, Sand: NPC-walkable.
  - Water: NPC-blocking.
- Building walkability:
  - Constructed Warehouse, TownHall, and Farm block NPC walking.
  - Fields remain NPC-walkable.
  - Blueprints follow their constructed kind for collision data.
- Resource nodes:
  - Block building placement.
  - Block NPC walkability in collision data.
  - When depleted and `ResourceNode` is removed, they stop blocking.
- Existing building/blueprint footprint overlap remains invalid.
- NPC movement systems do not consume collision data in v1.

## API, Bridge, And UI

- Add specific engine validation reasons such as out of bounds, overlaps
  building, invalid terrain, and blocked by resource node.
- Field placement batch previews should reject individual invalid cells while
  allowing valid cells.
- Godot bridge remains thin and asks Rust for placement validity/collision data.
- Godot UI keeps existing overlay-only feedback for v1.
- Collision data is derived from existing ECS state and definitions; no
  persistent collision map or save migration is required.
- Future pathfinding topology is deferred.

## Acceptance Criteria

- Buildings reject invalid terrain according to kind.
- Fields reject Sand and Water.
- Building placement rejects resource node overlap.
- Building placement still allows NPC overlap.
- Collision queries distinguish build-blocked from NPC-walk-blocked tiles.
- Major buildings and matching blueprints report NPC-walk-blocking; fields do
  not.
- Water and resource nodes report NPC-walk-blocking.
- Existing movement tests remain behaviorally unchanged.

## Key Decisions

- Simple solid terrain policy selected.
- Resource nodes block placement; NPCs do not.
- Movement behavior is unchanged in v1.
- Layered collision flags selected.
- Overlay-only UI feedback selected.
- Specific engine error reasons selected.
- Pathfinding topology deferred.

## Open Decisions

None blocking for this v1 brief. Future pathfinding still needs interaction-cell
rules for blocked resources/buildings.

## Repository Evidence

- `rust/game_engine/src/components.rs`: terrain, NPC, and movement components.
- `rust/game_engine/src/buildings.rs`: current building placement errors and
  validation.
- `rust/game_engine/src/farming.rs`: field placement validation.
- `rust/game_engine/src/movement.rs`: current movement only checks bounds.
- `rust/godot_bridge/src/world/game_world.rs`: current green/red placement
  preview.
