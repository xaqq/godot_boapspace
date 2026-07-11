# Resource Panel

## Summary

Add a Resources panel, following the existing Tasks and Housing panel pattern,
that summarizes the resource stock on the active surface. The panel shows live
usable and committed quantities for every resource, quick changes over fixed
historical periods, and a selectable daily history graph.

Historical quantities are simulation data. They are recorded deterministically
in `game_engine` per surface and are independent of Godot frame rate, simulation
speed, and whether the panel is open.

## Goals

- Let the player see the current usable quantity of every resource on the active
  surface at a glance.
- Distinguish resources that remain usable from resources committed to building
  blueprints.
- Show whether usable stock increased or decreased over 1, 7, 30, and 365
  simulated days.
- Let the player inspect the daily usable-stock history of one resource.
- Preserve isolation between surfaces.

## Non-goals

- Combining resources from multiple surfaces into a colony-wide total.
- Counting ungathered natural resource nodes as owned resources.
- Tracking the history of committed resources.
- Attributing changes to individual producers, consumers, transfers, or events.
- Showing gross production and consumption when they cancel out to the same net
  stock change.
- Forecasting future stock, capacity, shortages, or construction requirements.
- Issuing transfer, gathering, storage, or construction commands from the panel.
- Exporting historical data.
- Adding save/load support or save migration as part of this feature.
- Changing how existing full-size panels overlap or coordinate with one another.

## Resource Quantity Definitions

All quantities are scoped to the active surface.

### Usable stock (`Now`)

`Now` is the sum of resources in all owned inventories on the surface:

- NPC inventories.
- Completed warehouse inventories.
- Farm inventories.
- Forester lodge inventories.

It excludes:

- Quantities remaining in natural resource nodes.
- Resources deposited into building blueprints.

The aggregation must include each eligible inventory exactly once. A resource
kind with no usable stock still has a quantity of zero and remains visible.

### Committed stock (`Committed`)

`Committed` is the sum of resources already deposited into incomplete building
blueprints on the active surface. These resources are no longer included in
`Now` because they are no longer usable for another purpose.

Committed stock is a live supplementary value only. It has no delta columns and
is not recorded in the historical graph. If a blueprint or its construction
progress ceases to exist, including when construction completes, its deposited
resources cease contributing to `Committed`. This feature does not introduce
refund behavior.

Natural resource-node quantities are excluded from both values.

### Numeric range

Individual inventories continue using their existing `u32` quantities. Surface
aggregates, deltas, and recorded historical values use a representation wide
enough to sum all inventories without `u32` overflow; the feature contract uses
`u64` quantities and signed deltas.

## Historical Data

### Ownership and isolation

Each surface owns its own usable-stock history. History begins when that surface
is created and never reads or incorporates another surface. A newly created
surface has no history for dates before its creation, even though all surfaces
share the simulation's current world date.

The initial state is recorded as the surface's first daily point on its creation
day. One immutable sample is then recorded after simulation processing completes
for each subsequent simulated day. There can be at most one persisted daily
sample for a given surface and simulation day.

Samples contain the usable `Now` quantity for every `ResourceKind`; committed
stock is not sampled. Historical samples do not change when entities or
inventories later change or disappear.

### Timing

- Sampling follows simulated time, not rendered frames or calls from the panel.
- Each completed day produces exactly one sample after all simulation work
  belonging to that day has run.
- Pausing the simulation produces no new or duplicate samples.
- Higher simulation speeds may cross several day boundaries in one rendered
  frame, but must still produce exactly one sample for each crossed day.
- Opening or closing the Resources panel has no effect on sampling.

The panel also appends a non-persisted `Now` point to the displayed graph using
the current live usable quantity. This point represents the incomplete current
day and updates while the simulation runs.

### Retention and future persistence

Retain every daily sample from surface creation for the lifetime of the current
simulation session. Do not downsample or discard old points.

The project currently has no save/load system. Adding one is outside this
feature's scope, but a future save format must preserve per-surface resource
history so loading a game does not reset its graphs or comparisons.

## Quick Changes

For each resource, show the signed change in usable stock over these exact
lookback windows:

- `1d`: 1 simulated day.
- `7d`: 7 simulated days.
- `30d`: 30 simulated days; this is a fixed period, not a calendar month.
- `365d`: 365 simulated days; one simulation year.

Each change is the current live `Now` quantity minus the recorded usable
quantity for the exact target simulation day. If the surface has no sample for
that day, show an em dash (`—`) rather than comparing against zero or the oldest
available sample. Do not show percentages.

Delta presentation is:

- Positive: explicit `+` prefix and green text.
- Negative: minus prefix and red text.
- Zero: neutral text.
- Unavailable: muted `—`.

The colors supplement the sign and must not be the only way the direction is
communicated.

## User Interface

### Panel lifecycle

- Add a `Resources` button to the game header beside the existing Tasks and
  Housing buttons.
- The button toggles a full-map Resources overlay using the same overall shell,
  inset, header, and Close-button conventions as those panels.
- The panel starts hidden.
- Opening the panel does not pause or otherwise change the simulation.
- Preserve the existing independent overlay behavior: opening Resources does not
  close Tasks, Housing, or NPC Details, and opening those panels does not close
  Resources.
- While visible, live values and the graph's `Now` point refresh as simulation
  state changes. UI caching may avoid rebuilding unchanged content.
- Switching the active surface while the panel is open refreshes all values and
  history in place for the new surface.

### Overview table

Show one stable, selectable row for every value in `ResourceKind::ALL`, in its
defined order. Rows remain visible when both quantities are zero.

The columns are:

1. `Resource`: existing resource icon and label/tooltip.
2. `Now`: live usable stock.
3. `Committed`: live blueprint deposits.
4. `1d`.
5. `7d`.
6. `30d`.
7. `365d`.

The table is scrollable if the available panel size cannot contain all rows.
Resource rows are interactive controls; the reusable `ResourceQuantity` display
may be used within them, but panel-specific selection behavior does not belong
in that generic widget.

### Selection

No resource is selected when the panel is first created. Until the player
selects a row, the graph area displays a clear `Select a resource to view its
history` empty state.

Selecting a row displays that resource's graph and gives the row a visible
selected state. Clicking the selected row again does not deselect it. Once made,
the selection is retained when the panel is closed and reopened and when the
active surface changes; the graph contents simply switch to the selected
resource's history on the new surface.

### History graph

The graph displays only the selected resource's usable-stock history.

- Render a line graph with visible point markers.
- Plot simulation day on the X-axis and quantity on the Y-axis.
- Provide `30d`, `365d`, and `All` range controls; `30d` is the default.
- The selected range filters persisted daily samples and includes the live
  `Now` point.
- Hovering a persisted point shows its simulation day and exact quantity.
- Hovering the live point identifies it as `Now` and shows its exact quantity.
- Draw the live point distinctly from completed daily samples.
- With no completed daily sample in the selected range, show the live point
  without implying a historical trend.
- With one completed sample, show that sample and `Now`; do not fabricate
  intermediate values.
- The graph must resize with the panel without clipping axes, controls, or hover
  details.

The graph is implemented through Rust-backed Godot controls; no GDScript is
introduced.

## Architecture and Data Contracts

### `game_engine`

Durable rules and history belong in `game_engine`:

- A pure, per-surface resource overview query returns usable and committed
  aggregates for every `ResourceKind`.
- Per-surface simulation state owns daily usable-stock samples.
- Simulation scheduling records samples at deterministic day boundaries.
- History/query APIs expose typed values and do not depend on Godot nodes,
  rendered frames, or UI visibility.

This data remains isolated within each surface's Bevy `World`, consistent with
the existing `SurfaceRuntime` model.

### Godot bridge and scene

The bridge remains thin. It obtains typed overview/history view data for the
currently rendered surface and presents it through a Rust `GodotClass` panel and
Rust-backed graph control.

The new scene and its Rust module follow the existing panel directory symmetry.
Node references use typed exported `Gd<T>` / `OnEditor<Gd<T>>` fields and typed
signal connections. Resource icons load through Godot's `ResourceLoader` path,
reusing the existing resource assets and tooltip behavior.

The panel may poll while visible and cache typed view data, matching the Tasks
and Housing panels. It must not rely solely on the existing `resources_changed`
signal, which does not currently represent every inventory mutation.

## Edge Cases and Failure Behavior

- An empty surface shows every resource with `Now = 0`, `Committed = 0`, and
  unavailable deltas where exact baselines do not exist.
- Creating, removing, or mutating an eligible inventory updates `Now` live and
  affects only future daily samples.
- Depositing into a blueprint reduces `Now` and increases `Committed`; only the
  usable-stock reduction is represented in deltas and history.
- Completing or removing a blueprint removes its contribution from `Committed`
  without rewriting history.
- Same-day gains and losses that result in the same sampled quantity appear as a
  zero net change; turnover tracking is intentionally out of scope.
- Switching to a surface without enough history shows `—` for unavailable
  comparisons and displays only the history owned by that surface.
- A future `ResourceKind` remains visible through the resource-kind iteration
  contract; historical samples that predate support for it are treated as zero
  only when a future compatibility/migration layer explicitly establishes that
  default.

## Acceptance Criteria

### Simulation and data

- Usable totals sum NPC, warehouse, farm, and forester lodge inventories exactly
  once and exclude resource nodes and construction deposits.
- Committed totals sum deposited construction resources and exclude all usable
  inventories and resource nodes.
- Aggregates safely exceed `u32::MAX` without overflow or truncation.
- Resource totals and history from one surface never affect another surface.
- A surface records its initial point and at most one immutable point for each
  subsequent simulated day.
- Pausing produces no samples; all supported speed multipliers produce identical
  samples for identical simulation state and elapsed simulated time.
- Crossing multiple days during accelerated simulation records every crossed
  day without gaps or duplicates.
- Quick changes use exact 1/7/30/365-day baselines and return unavailable when an
  exact baseline is absent.
- Removing or changing entities does not mutate previously recorded samples.

### Bridge and UI

- The Resources header button opens and closes the panel, and Close hides it.
- Opening the panel leaves simulation play state and speed unchanged.
- All resource kinds appear in stable order, including zero-quantity resources.
- `Now`, `Committed`, and all available deltas refresh while the panel is visible
  and after an active-surface change.
- Delta signs, colors, neutral zero state, and muted unavailable state follow the
  specified presentation.
- The initial graph state asks the player to select a resource.
- Selecting a resource highlights its row and displays only its history.
- Selection persists across panel toggles and surface changes.
- The graph range controls, axes, point markers, hover details, sparse-data
  behavior, live point, and resizing behave as specified.
- The Godot project starts headlessly with all typed scene references resolving.

## Validation Expectations

- Add `game_engine` unit or integration tests for aggregation membership,
  committed resources, overflow, sampling boundaries, pause/speed behavior,
  exact lookbacks, retention, entity lifecycle, and surface isolation.
- Add bridge-side unit tests for typed panel view rows, stable resource ordering,
  delta formatting/state, range filtering, and graph coordinate helpers where
  those helpers are pure Rust.
- Validate the Resource panel scene and interactive behavior through an
  appropriate Godot headless run in addition to the Rust workspace test suite.

## Open Decisions

None.
