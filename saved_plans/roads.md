# Roads

## Summary

Introduce player-built roads as surface-local infrastructure. Roads accelerate
ground movement, and all travel-related simulation decisions account for their
reduced travel time. Players lay road blueprints by dragging across the map;
NPCs deliver any required materials and then perform the construction labor.

Roads are a distinct per-cell overlay. They do not replace the underlying
terrain and are not ordinary buildings, although they use the shared
construction workflow.

## Goals

- Let players create road networks that materially improve colony travel times.
- Provide three visually and mechanically distinct road tiers.
- Make NPC routing, job selection, and logistics prefer genuinely faster travel,
  even when a road route contains more cells.
- Make road placement efficient for long, cardinally connected runs.
- Reuse a consistent material-and-labor construction model for roads and all
  other building blueprints.
- Keep road state and behavior isolated per surface.

## Non-goals

- Diagonal road connections.
- Bridges, roads on water, tunnels, or roads between surfaces.
- Road wear, maintenance, damage, or weather effects.
- Canceling road blueprints, demolishing completed roads, downgrading roads, or
  refunding materials.
- Introducing save/load support. The project has no persistence system yet;
  road state must be included when persistence is added later.

## Road tiers

Each road occupies exactly one grid cell. Costs are per cell and an upgrade pays
the full listed cost.

| Tier | Player-facing name | Material cost | Ground movement multiplier |
| --- | --- | --- | --- |
| 1 | Dirt Path | None | 1.5x |
| 2 | Cobblestone Road | 1 Stone | 2x |
| 3 | Flagstone Road | 1 Stone Block | 3x |

The chosen names distinguish the tiers more clearly than the original
`Dirt Road / Stone Road / Paved Stone Road` wording. Other alternatives
considered were `Dirt Track / Stone Road / Paved Stone Road` and
`Dirt Road / Stone Road / Cut-Stone Road`.

## Construction rules

Roads use the same blueprint, material-delivery, task, and labor concepts as
other construction. To support the free Dirt Path without instant completion,
construction gains a generic labor requirement that also applies to existing
building blueprints.

- A blueprint's required labor is 180 simulation work ticks per footprint cell,
  equivalent to three in-game hours per cell.
- NPCs may begin labor only after every required material has been deposited.
- At most one NPC contributes labor to a given blueprint during a simulation
  tick.
- A working NPC contributes one work tick per simulation tick while at a valid
  interaction cell.
- Labor progress is cumulative and remains intact when the worker is
  interrupted, reassigned, becomes hungry, or temporarily cannot reach the
  blueprint.
- Construction completes only when both its material and labor requirements are
  satisfied.
- Dirt Paths require no material delivery but still require labor; they must not
  auto-complete.
- Road blueprints remain walkable. A new road blueprint provides ordinary
  terrain speed until it completes.
- A road worker may stand on the road blueprint cell to perform labor, following
  the existing interaction behavior for walkable one-cell construction.
- A blueprint waits indefinitely when required material or a reachable worker
  is unavailable. It does not fail or disappear.
- Pausing the simulation stops material hauling, labor, movement, and
  completion. Simulation speed changes only how quickly normal simulation ticks
  are processed.

The shared construction UI must distinguish deposited materials from labor
progress for every blueprint affected by this generic labor requirement.

## Placement and occupancy

### Placement gesture

The Roads panel starts a persistent placement mode for the selected tier.

- A single click proposes one road cell.
- Holding and dragging proposes a freehand stroke.
- When pointer samples skip cells, connect consecutive samples cardinally by
  filling the horizontal segment first and the vertical segment second. This
  deterministic elbow prevents gaps and diagonal connections.
- Suppress duplicate cells while preserving the stroke's first-occurrence
  order.
- Releasing the pointer submits the stroke but leaves the same road tool active.
- Right-click or Escape cancels placement mode without affecting already placed
  blueprints.
- Switching the rendered surface cancels placement mode and clears its preview,
  matching the existing surface-switch behavior.

### Atomic validation

Validate a stroke as one batch against the state that existed before the stroke.
If any proposed cell is invalid, reject the entire stroke and create no
blueprints. The preview and Roads panel identify every invalid cell and its
reason. A valid stroke previews and reports its aggregate material cost before
placement.

Within one stroke, duplicate cells are ignored rather than treated as an error.
Every remaining cell must be independently valid, including upgrades mixed
with new road cells.

### Valid cells

Roads and road blueprints are permitted only on Grass, Dirt, or Sand terrain.
NPC occupancy does not prevent placement.

Reject a proposed cell when it:

- is outside the current surface grid;
- is Water or has no valid underlying tile;
- contains a resource node;
- overlaps any building, building blueprint, Field, Tree Plot, or their
  blueprints;
- already contains a road blueprint or pending road upgrade;
- contains a completed road of the same or a higher tier; or
- otherwise cannot support walkable ground movement.

Road cells are dedicated infrastructure cells. New buildings, building
blueprints, Fields, Tree Plots, and resource-node placement must likewise reject
cells containing a completed road, road blueprint, or pending upgrade.

## Road upgrades

A higher tier may be placed directly over a completed lower-tier road.

- The upgrade is represented as pending construction on that cell while the
  completed lower-tier road remains present.
- The existing road remains walkable and retains its old movement multiplier
  until the upgrade completes.
- The upgrade charges the higher tier's full per-cell material cost and full
  180-tick labor requirement.
- The upgrade follows the same material-first, single-worker labor rules as new
  roads.
- When complete, it atomically replaces the lower tier and invalidates the
  surface's traversal-cost snapshot.
- Equal-tier replacement and downgrade placement are invalid.
- A cell cannot have more than one pending road construction or upgrade.

## Movement and pathfinding

### Movement benefit

Road multipliers apply to every simulated ground mover that uses the shared
ground-navigation model. NPCs are the only such movers currently, but the rule
must not be encoded as NPC-specific behavior.

Movement between two cardinal cell centers uses the completed road multiplier
of the destination cell for the entire step. If the destination has no
completed road, it uses the normal ground multiplier. A new blueprint gives no
bonus; a pending upgrade continues to use its completed lower-tier multiplier.

Intrinsic mover speed and the destination multiplier together determine the
effective per-tick speed. The road must not permanently mutate the mover's base
maximum velocity.

### Weighted travel time

All ground navigation uses deterministic weighted travel time rather than raw
cardinal cell count. The cost of entering a cell is derived from the reciprocal
of its movement multiplier, using deterministic integer or fixed-point weights
that exactly preserve the 1x, 1.5x, 2x, and 3x ratios.

Weighted cost applies consistently to:

- route shape;
- selection among reachable destination or interaction cells;
- construction and other job selection;
- resource, inventory, and logistics source selection; and
- any future ground-travel comparison built on the shared navigation API.

The fastest estimated travel-time route wins even when it contains more cells.
Equal-cost outcomes preserve the existing deterministic behavior: destination
ties use lower row then lower column, and path expansion retains the current
fixed north, west, east, south ordering.

The surface-local navigation snapshot must include both walkability and
traversal weights in its change fingerprint. Completing or upgrading a road
therefore causes active routes and cached travel distances on that surface to
replan on the next navigation update. Blueprint placement, material deposits,
and labor progress do not alter traversal cost and must not trigger a cost
change. No road change may affect another surface.

## Simulation data and architecture

Durable road rules belong in `game_engine`; Godot is responsible only for input,
presentation, and querying simulation state.

The simulation needs surface-local concepts equivalent to:

- a road tier definition containing its label, per-cell cost, movement
  multiplier, and upgrade ordering;
- one road state keyed by cell coordinate, with either a completed tier or a
  completed tier plus pending higher-tier upgrade;
- road blueprint material and labor progress;
- atomic batch placement validation and placement results; and
- traversal-cost queries and a navigation revision that changes when effective
  road speed changes.

The stable logical identity of a road is its `(SurfaceId, CellCoord)`. Road
connectivity and movement effects never cross surfaces.

Road construction may reuse shared construction components and tasks, but roads
must not be forced into `BuildingKind` if doing so would inherit inappropriate
building collision, footprint, selection, or rendering behavior. The bridge
must expose typed commands and read-only view data rather than implementing any
of these rules itself.

## Godot bridge and user interface

### Roads panel

Add a dedicated, always-visible `Roads` panel to the left sidebar immediately
below the existing Build palette. It contains typed buttons for Dirt Path,
Cobblestone Road, and Flagstone Road.

The panel shows:

- each tier's per-cell material cost and movement multiplier;
- the active road tool;
- the right-click/Escape cancellation hint;
- the current stroke's cell count and aggregate material cost; and
- all blocking validation reasons when the current atomic stroke is invalid.

The map preview marks every proposed cell valid or invalid before release. An
invalid atomic stroke must make it clear that none of its cells will be placed.

### Tile inspection

Roads remain part of normal tile selection rather than becoming selectable
major buildings. Tile Info shows, when applicable:

- completed road tier and effective movement multiplier;
- whether the cell contains a new blueprint or an upgrade;
- the target tier;
- deposited versus required materials;
- completed versus required labor; and
- that the old tier remains effective during an upgrade.

### Typed integration

Use strongly typed exported node references and direct Rust method/signal calls.
Do not introduce GDScript or string-based dynamic calls for road controls.

## Rendering and assets

Add a dedicated typed `TileMapLayer` for roads. It renders above base terrain
and below resource nodes, buildings, plots, and NPCs. It is populated only from
the currently rendered surface and refreshed when that surface changes.

Road connections are cardinal only. Each tier requires a 64x64-tile, 4x4 atlas
containing all 16 north/east/south/west connectivity masks:

- one isolated tile;
- four endpoints;
- two straights;
- four corners;
- four T-junctions; and
- one four-way crossroad.

Atlas mapping must be explicit and deterministic. All required textures are
imported Godot assets loaded as `Texture2D` resources through `ResourceLoader`;
runtime filesystem image loading is not permitted.

Completed connectivity considers every cardinally adjacent completed road,
regardless of tier. Each cell retains the artwork for its own tier, so different
materials meet at a direct seam without extra transition assets.

Blueprints use the target tier's road art with the existing cyan blueprint
tint. Their displayed connectivity considers adjacent completed roads and
adjacent road blueprints so the planned network is visible. Completed-road
connectivity ignores blueprints until they finish. Completing or upgrading a
road refreshes the changed cell and its four cardinal neighbors.

## Failure and edge-case behavior

- Missing materials or workers leave a valid blueprint waiting with visible
  progress; they do not invalidate the blueprint.
- If a worker or its route disappears during labor, progress remains and the
  construction task becomes available again.
- If the underlying simulation state changes between preview and release, the
  stroke is revalidated atomically at submission time.
- Two placement commands targeting the same cell are resolved in deterministic
  simulation order; the later command observes the earlier accepted state and
  is rejected.
- A completed road or upgrade changes movement only after construction
  completion has been applied by the simulation.
- A mover already in transit observes the destination-cell rule from current
  completed simulation state; the route is replanned when the navigation
  revision changes.
- Empty colonies can place blueprints, but no labor occurs until a worker is
  available.
- Road state, construction, routing revisions, rendering, and placement operate
  independently on each surface.

## Acceptance criteria

### Construction and placement

- Each tier creates the correct per-cell blueprint and charges the correct
  aggregate material cost for a valid stroke.
- Dirt Paths remain blueprints until an NPC performs 180 labor ticks per cell.
- Paid roads accept labor only after all materials have arrived.
- Existing buildings and other blueprints also require 180 labor ticks per
  footprint cell after their materials arrive.
- Only one NPC advances a blueprint's labor in a tick, and interruption does not
  erase progress.
- Fast pointer movement produces a continuous horizontal-then-vertical
  cardinal stroke with no duplicate cells.
- One invalid cell rejects the complete stroke, reports every invalid cell and
  reason, and creates no road state.
- Single-click placement, persistent placement mode, right-click/Escape cancel,
  and surface-switch cancellation behave as specified.
- Missing materials or workers leave visible, stable waiting blueprints.

### Occupancy and upgrades

- Roads accept Grass, Dirt, and Sand and reject Water, resource nodes, all
  building/plot states, and out-of-bounds cells.
- Other placeable infrastructure rejects every road or road-blueprint state.
- A higher tier can upgrade a completed lower tier for full cost and labor.
- The lower-tier multiplier remains active until the upgrade finishes.
- Equal-tier placement, downgrade placement, and placement over a pending road
  operation are rejected.

### Movement and navigation

- Measured center-to-center movement uses 1.5x, 2x, and 3x speed for the three
  completed tiers and normal speed for unbuilt blueprints.
- Boundary behavior uses the destination cell's multiplier.
- Weighted routing selects a longer-in-cells road route when its total travel
  time is lower.
- Destination, job, construction, and logistics/source selection use the same
  weighted travel cost.
- Equal-cost routes and targets remain deterministic.
- Completing or upgrading a road invalidates cached costs and replans active
  routes; mere blueprint progress does not.
- A road on one surface has no movement, routing, or selection effect on any
  other surface.

### Rendering and UI

- The Roads panel exposes all three typed tools and the specified live feedback.
- Tile Info reports completed roads, new blueprints, and upgrades with material
  and labor progress.
- Every isolated, endpoint, straight, corner, T-junction, and crossroad mask maps
  to the expected atlas tile for all three tiers.
- Mixed tiers connect without visual gaps, while each cell retains its own
  material.
- Blueprint masks include planned and completed neighbors; completed masks do
  not include unfinished neighbors.
- Completing a road refreshes its own cell and all affected cardinal neighbors.
- The Godot scene loads headlessly with typed Roads panel and road-layer
  references, and all road textures load through the imported-resource pipeline.

## Explicit assumptions

- One simulation tick represents one in-game minute, so 180 work ticks equal
  three in-game hours.
- Ordinary traversable terrain has a 1x movement multiplier.
- NPC presence never blocks infrastructure placement.
- Road multipliers affect movement time only; they do not change hunger,
  inventory, work rate, or construction labor directly.
- No persistence compatibility or migration is required until a save/load
  system exists.
