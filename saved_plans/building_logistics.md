# Building logistics

## Summary

Introduce two storage tiers, configurable logistics buildings, surface-unique
building names, separate refinery supply and production work, and visible
wheelbarrow hauling. Durable rules and state belong to `game_engine`; Godot
provides typed controls, rendering, and live views of the simulation.

## Goals

- Give players explicit control over whether storage and refinement buildings
  participate in logistics.
- Let storage pull finished goods from refineries and let refineries choose
  between storage-supplied inputs and direct gathering/producer inventories.
- Separate unskilled input supply from skilled refinery production.
- Make storage-connected hauling faster and visible through wheelbarrows.
- Give every building a stable, user-editable identity within its surface.
- Replace technical building hover details with useful operational summaries.

## Non-goals

- Save/load, migrations, and serialization are deferred because the project has
  no persistence subsystem yet.
- Manual links between individual source and destination buildings are not part
  of this feature.
- Configurable logistics radii, storage quotas, and per-resource capacity
  partitions are not included.
- NPC death/despawn cargo recovery is deferred because no such lifecycle exists
  today.
- Activity controls for farms, lodges, housing, fields, tree plots, Town Hall,
  or construction blueprints are not included.

## Storage buildings

### Depot

- Add a `Depot` building as the smaller storage tier.
- Footprint: 2x2.
- Construction cost: 20 Wood and 10 Stone.
- Inventory capacity: 500 total units.
- It has the same allowed-deposit filter, pull controls, active-state behavior,
  and logistics capabilities as Warehouse.
- Place Depot immediately before Warehouse in the building palette.
- Provide new footprint-matched 128x128 building art.

### Warehouse

- Change the existing Warehouse footprint from 2x2 to 4x4.
- Retain its current construction cost of 40 Planks and 20 Stone Blocks.
- Retain its current inventory capacity of 2,000 units.
- Replace its current art with footprint-matched 256x256 art.

Both storage kinds continue to block navigation across their footprints and use
the normal building placement, terrain, collision, construction, inventory, and
resource-overview rules.

## Building names

- Assign a default name when a blueprint is placed, using
  `{BuildingKind label} #{number}`, starting at 1.
- Maintain a separate monotonically increasing counter per building kind and
  surface. Deleted numbers are never reused.
- If the next generated name already exists because of a manual rename, advance
  until an unused name is found.
- Names must be unique across all blueprints and completed buildings on the same
  surface. Other surfaces do not participate in validation.
- Compare uniqueness case-insensitively after trimming leading and trailing
  whitespace while preserving the user's committed casing for display.
- A valid name contains 1 through 64 Unicode scalar values after trimming.
- Blueprints and completed buildings can both be renamed.
- The building panel provides a text field, an Apply button, and Enter as a
  commit shortcut. Invalid or duplicate input leaves the previous name
  unchanged and displays an inline error.
- Hover tooltips display the custom name. Names are not rendered persistently
  over the map.

## Active state

- Completed Depot, Warehouse, Sawmill, Stoneworks, and Kitchen buildings have an
  Active control and default to active.
- Blueprints cannot be activated or deactivated.
- Pull/filter/name configuration remains editable while a building is inactive
  and takes effect when it is reactivated.
- Deactivation takes effect immediately:
  - Cancel assigned work and release its reservations.
  - Stop creating or assigning new work involving the building.
  - Preserve refinery input/output buffers, current recipe, and partial
    production progress.
  - Preserve storage contents.
  - Leave normal cargo already carried by an NPC with that NPC.
- An inactive Depot or Warehouse cannot accept deposits or serve as a source for
  food, construction, refinery supply, storage pulls, or any other logistics.
- An inactive refinery neither produces nor accepts input and neither of its
  buffers may serve as a source.
- Resources in inactive storage and refinery buffers remain owned and continue
  to appear in colony resource totals and history.

## Pull controls

All pull controls default off.

### Depot and Warehouse: Pull from Refineries

- Add one `Pull from Refineries` checkbox for every resource kind currently
  produced by a refinery: Planks, Stone Blocks, and Food.
- An enabled resource may be hauled only from the output buffer of an active
  compatible refinery on the same surface.
- Enabling Pull also enables the existing Allowed Deposits setting for that
  resource.
- Disabling Allowed Deposits automatically disables Pull for that resource.
- Pull attempts to fill all currently available storage capacity. Multiple
  workers may haul concurrently when reservations prove that source stock and
  destination capacity are available.

### Refinery: Pull from Storage

- Add one `Pull from Storage` checkbox for each input resource supported by the
  refinery's recipes.
- When enabled, supply work uses active Depots and Warehouses exclusively.
- When disabled, storage is excluded. Supply work may gather the relevant
  natural resource or haul it directly from an appropriate Farm or Forester
  Lodge inventory.
- Supply attempts to fill all available compatible input-buffer capacity.

### Automatic matching and determinism

- Pull settings match any compatible source on the same surface; players do not
  select individual source buildings.
- Prefer the nearest reachable candidate.
- Use stable entity-ID ordering to break equal-distance ties.
- Reserve both source quantity and destination capacity before assigning work.
- Full, empty, unreachable, removed, or deactivated endpoints release their
  reservations and may be retried on a later tick without duplicating or
  destroying resources.

## Refinery task model

Replace the current combined source/haul/process behavior with two responsibilities:

1. Unskilled supply work fills refinery input buffers according to each input's
   pull mode. A supply worker does not perform production.
2. Skilled production work is assignable only when a compatible input unit is
   already buffered and the output buffer has room. The skilled worker consumes
   and processes one unit using the existing recipe duration and skill rules.

The work priority order is:

1. Urgent food acquisition.
2. Recovery of a loaded wheelbarrow.
3. Construction logistics.
4. Skilled refinery production.
5. Refinery supply and storage pull hauling.
6. Farming and forestry work.
7. Idle behavior.

Existing deterministic scheduling and reservation guarantees must be preserved.

## Wheelbarrows

### Simulation behavior

- Any haul whose source or destination is a Depot or Warehouse uses a
  simulation-owned wheelbarrow.
- A wheelbarrow carries one resource kind and at most 25 units.
- For that job it replaces the NPC's normal five-unit carried-resource cargo;
  the two cargo mechanisms are not used simultaneously.
- Attach an empty wheelbarrow when the worker is assigned, including while the
  worker travels toward the source.
- Remove an empty wheelbarrow after successful delivery.
- If an unladen job is canceled or its source becomes invalid, remove the empty
  wheelbarrow immediately.
- If a loaded destination becomes inactive, disappears, becomes full, or is
  otherwise invalid, schedule recovery after urgent food needs:
  - Reroute the load to the nearest reachable active storage building that
    allows the resource and has reserved capacity.
  - If no recovery destination exists, retain the loaded wheelbarrow on the NPC
    and retry later.
- Wheelbarrow contents count as owned resources in surface resource totals.

### Rendering and assets

- Render the wheelbarrow as a shared animated child overlay usable by every NPC
  appearance rather than duplicating each NPC sprite sheet.
- Provide eight-direction animation sets for:
  - An empty wheelbarrow.
  - A loaded wheelbarrow for each of the eight `ResourceKind` values.
- Use directional walking animation while moving and the first directional
  frame while stationary.
- Hide the existing normal cargo icon while a wheelbarrow is equipped.
- Load all assets through Godot's imported-resource pipeline.

## Building panel and hover UI

The typed Rust `BuildingInfoPanel` remains the control surface. It gains:

- The rename field, Apply action, and inline validation error.
- An Active control for completed logistics buildings.
- Existing Allowed Deposits controls for both storage kinds.
- Storage `Pull from Refineries` controls with the dependency described above.
- Per-input refinery `Pull from Storage` controls.
- Immediate rollback/re-query and player-visible feedback when a command is
  rejected because its entity disappeared, belongs to another surface, or is
  no longer eligible.

Remove cell coordinates and footprint dimensions from building hover tooltips.
Show contextual summaries instead:

- Blueprint: custom name, building type, and construction progress.
- Depot/Warehouse: custom name, type, active state, used/maximum capacity,
  nonzero per-resource stock, compact allowed-deposit list, and compact
  refinery-pull list.
- Refinery: custom name, type, active state, every supported input/output kind
  and quantity including zero, and each input's pull state.
- Other completed buildings: custom name, type, and existing relevant status
  such as housing occupancy.

Tooltips continue to refresh from the rendered surface while open.

## Architecture and interface requirements

- Store building names, per-surface counters, activity state, pull
  configuration, reservations, and wheelbarrow cargo in `game_engine` ECS
  components/resources.
- Extend the surface-scoped `GameSimulation` command/query pattern used by the
  warehouse whitelist for rename, activity, and pull settings.
- Commands reject missing entities, wrong-surface entities, blueprints where a
  completed building is required, and unsupported building/resource pairs
  without partial mutation.
- Keep `godot_bridge` thin: decode the selected entity, call typed simulation
  methods, and render typed view data.
- Use typed Godot node references and direct method/signal calls. Do not add
  GDScript or stringly typed method dispatch.
- Cross-surface names, sources, destinations, hauling, and reservations are
  forbidden.

## Relevant implementation areas

- `rust/game_engine/src/buildings.rs`: building definitions, names, activity,
  storage construction, and inventory capacities.
- `rust/game_engine/src/refining.rs` and `rust/game_engine/src/logistics.rs`:
  task separation, endpoints, reservations, pull matching, wheelbarrows, and
  recovery.
- `rust/game_engine/src/systems.rs` and `rust/game_engine/src/simulation.rs`:
  schedule ordering and surface-scoped commands.
- `rust/godot_bridge/src/panel/building_info_panel.rs` and its scene: typed
  controls and live view data.
- `rust/godot_bridge/src/panel/map_entity_tooltip_panel.rs`: contextual hover
  summaries.
- `rust/godot_bridge/src/world/game_world.rs`, the building palette, NPC scenes,
  and generated assets: placement methods, texture mappings, and wheelbarrow
  rendering.

## Tests and validation

### Simulation tests

- Depot and Warehouse definitions, costs, footprints, capacities, completion
  components, placement, collision, navigation blocking, and surface isolation.
- Default name generation, per-kind counters, skipped collisions, no number
  reuse, Unicode length, trimming, case-insensitive uniqueness, rename errors,
  blueprint renaming, and wrong-surface rejection.
- Active defaults and exclusion from every deposit, withdrawal, production, and
  hauling path.
- Immediate deactivation during supply, hauling, and partial refinery
  production, including reservation release and cargo conservation.
- Pull defaults, strict refinery storage mode, direct-source mode, storage
  refinery-output pulls, Allowed Deposits dependency, full destinations,
  competing workers, multiple destinations, unreachable endpoints, and
  deterministic tie-breaking.
- Separate unskilled supply and skilled production eligibility.
- Wheelbarrow single-resource and 25-unit limits, attachment/removal, source and
  destination invalidation, recovery routing, loaded retention, and resource
  totals.

### Bridge and UI tests

- Depot palette position, placement method, asset mapping, and Warehouse art
  mapping.
- Typed rename, activity, filter, and pull controls, including rollback and
  inline errors.
- Live contextual tooltip content for blueprints, both storage kinds,
  refineries, inactive buildings, and other completed buildings.
- Wheelbarrow empty/loaded/facing visual selection and cargo-icon suppression.

### Validation commands

```bash
cargo test --manifest-path rust/Cargo.toml
cargo build --manifest-path rust/Cargo.toml
~/.local/bin/godot4 --headless --path godot --quit-after 2
```

## Acceptance criteria

- Players can build the correctly sized and priced Depot and enlarged Warehouse
  with footprint-matched visuals.
- Every blueprint and building has a valid, surface-unique name that can be
  edited with clear validation feedback.
- Activity controls immediately and safely isolate completed storage and
  refinement buildings without losing stock or production progress.
- Storage and refinery pull settings produce only the selected source behavior,
  fill available capacity through reserved deterministic hauls, and never cross
  surfaces.
- Refinery supply is performed independently from skilled production.
- Storage-connected haulers visibly use 25-unit wheelbarrows, recover invalid
  loaded hauls without resource loss, and shed empty wheelbarrows on completion.
- Hover tooltips show live operational information and no longer show cell or
  footprint details.
- All Rust tests pass, the workspace builds, and the Godot project starts
  headlessly without scene/resource errors.

## Open questions

None.
