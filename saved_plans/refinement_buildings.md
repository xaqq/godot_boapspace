# Refinement buildings

## Summary

Introduce an automated, NPC-operated refinement chain that separates gathered or
harvested raw materials from construction materials and edible Food.

The feature adds four resources:

- **Crops**, harvested from Fields.
- **Wild Berries**, gathered from natural resource nodes.
- **Planks**, produced from Wood at a Sawmill.
- **Stone Blocks**, produced from Stone at Stoneworks.

`Food` remains the only edible resource. Kitchens turn either Crops or Wild
Berries into Food. Existing buildings use Planks and Stone Blocks instead of raw
Wood and Stone, while the Sawmill and Stoneworks remain bootstrap buildings that
can be constructed from raw materials.

All durable production, logistics, reservation, skill, and task rules belong in
`game_engine`. Godot remains responsible for typed commands, queries, UI,
rendering, and imported assets.

## Goals

- Add a meaningful raw-to-refined production chain for construction and food.
- Make refining automatic but dependent on an eligible NPC, available input,
  travel, processing time, and output capacity.
- Allow NPCs to transport resources between natural nodes, building inventories,
  their own inventories, refineries, construction sites, and food consumers.
- Add distinct eligibility tags and skills for the three refining activities.
- Expose refining state, progress, tasks, resources, and blocking conditions to
  the player.
- Add resource, building, and NPC activity art for the new behavior.
- Preserve deterministic, surface-isolated simulation behavior.

## Non-goals

- Player-authored production orders, target quantities, recipe selection, or
  enable/disable toggles.
- Automatically moving finished products into Warehouses merely for stocking.
- Resource quality, variable recipe efficiency, by-products, or skill-based
  speed/yield modifiers.
- Technology unlocks or affordability-based palette button disabling.
- Building demolition, production cancellation, or construction refunds.
- Save/load support or migration; the project has no save system yet.
- New Field growth-stage art or new NPC appearances.

## Terminology and resources

The complete resource set is:

| Resource | Category | Natural node | Edible | Primary source/use |
| --- | --- | --- | --- | --- |
| Wood | Raw | Yes | No | Gathered; Sawmill input and bootstrap construction |
| Stone | Raw | Yes | No | Gathered; Stoneworks input and bootstrap construction |
| Wild Berries | Raw food ingredient | Yes | No | Gathered; Kitchen input |
| Crops | Raw food ingredient | No | No | Harvested from Fields; Kitchen input |
| Planks | Refined construction | No | No | Produced by Sawmill |
| Stone Blocks | Refined construction | No | No | Produced by Stoneworks |
| Food | Finished food | No | Yes | Produced by Kitchen and consumed by NPCs |
| Gold | Raw/valuable | Yes | No | Gathered; existing construction use |

Existing Godot-facing numeric discriminants for Wood, Stone, Food, and Gold must
remain stable. New resource kinds are appended rather than inserted between
existing values. Resource containers, totals, history, construction progress,
tooltips, and iteration must support all eight kinds without relying on a
four-element positional model.

Natural resource generation uses an explicit natural-kind set containing Wood,
Stone, Wild Berries, and Gold. Refined resources and Crops never spawn naturally.
The existing deterministic four-way generation mapping is retained, with the old
Food-node category becoming Wild Berries, so existing resource positions and the
other three categories remain stable for the same surface.

Every resource occupies one inventory-capacity unit, preserving the current
inventory sizing rule.

## Buildings

### New building definitions

| Building | Footprint | Construction cost | Input buffer | Output buffer |
| --- | --- | --- | --- | --- |
| Sawmill | 2x2 | 20 Wood, 10 Stone | 100 total Wood | 100 Planks |
| Stoneworks | 2x2 | 20 Wood, 20 Stone | 100 total Stone | 100 Stone Blocks |
| Kitchen | 2x2 | 20 Planks, 10 Stone Blocks | 100 total Crops/Wild Berries | 100 Food |

Each input and output buffer has its own capacity. Kitchen Crops and Wild Berries
share the 100-unit input capacity.

The three buildings use ordinary major-building placement rules: their full
footprints must be in bounds, on allowed terrain, free of resource nodes, and
must not overlap another blueprint or finished building. They block walking once
constructed. Refining is performed from a cardinally adjacent walkable cell;
workers are never required to stand inside a finished blocking footprint.

Production becomes available automatically when construction completes. A
finished output remains in its producer's output buffer until another job needs
it; no background job moves it to a Warehouse solely for stocking.

### Updated existing construction costs

For every existing building, each previous Wood unit becomes one Plank and each
previous Stone unit becomes one Stone Block. Existing Gold quantities do not
change.

| Building | Updated construction cost |
| --- | --- |
| Warehouse | 40 Planks, 20 Stone Blocks |
| Town Hall | 80 Planks, 60 Stone Blocks, 20 Gold |
| Farm | 20 Planks, 30 Stone Blocks |
| Field | 5 Planks, 1 Stone Block |
| Forester's Lodge | 20 Planks, 30 Stone Blocks |
| Tree Plot | 5 Planks, 1 Stone Block |
| Small House | 10 Planks, 5 Stone Blocks |
| Medium House | 30 Planks, 15 Stone Blocks |
| Large House | 60 Planks, 30 Stone Blocks |

The Sawmill and Stoneworks are the only buildings in this feature whose costs
remain raw Wood and Stone, ensuring the refinement chain can be bootstrapped.

## Recipes and production lifecycle

All recipes process one unit at a time and require 60 simulation ticks of work:

| Building | Input | Output | Duration |
| --- | --- | --- | --- |
| Sawmill | 1 Wood | 1 Plank | 60 ticks |
| Stoneworks | 1 Stone | 1 Stone Block | 60 ticks |
| Kitchen | 1 Crop | 1 Food | 60 ticks |
| Kitchen | 1 Wild Berries | 1 Food | 60 ticks |

Production is automatic. A refinery exposes one actionable task whenever its
input buffer contains a valid input or an external source can supply one, it has
output capacity, and it is not already claimed by another refining worker. Only
one worker and one batch may be active at a refinery at a time.

The production lifecycle is:

1. Claim the refinery task, one input unit, and one output slot. Prefer an
   already-buffered valid input before searching for an external source. Claims
   prevent two workers from relying on the same stock or capacity.
2. If the input is external, travel to its source. For a natural node, gather one
   unit using the existing gathering duration and rules. For an inventory,
   withdraw the reserved unit when the worker reaches an interaction cell.
3. Carry an external input to a cardinally adjacent walkable interaction cell at
   the refinery and deposit it into the refinery's input buffer. Skip the source
   and delivery phases when consuming an input already in that buffer.
4. Consume one input when processing begins and record the active recipe and
   progress on the refinery.
5. Advance progress only while an eligible worker is present and actively
   processing.
6. After 60 processing ticks, add one output, clear the batch state, and grant
   one matching skill XP to the worker who completes the unit.

The Kitchen has no semantic preference between Crops and Wild Berries. It chooses
the nearest eligible source of either ingredient. Equal distances are resolved
by source entity ID and then a stable resource-kind ordering solely as a final
deterministic tie-breaker.

Production does not begin if the output buffer is full. If output capacity
unexpectedly disappears after input was consumed, completed progress waits
without losing the input or creating output until a slot is available.

Processing progress belongs to the refinery, not the worker. Hunger may interrupt
the worker after processing has started; progress remains, the worker claim is
released, and the same or another eligible worker may resume it. Progress does
not advance without an eligible worker actively present.

Before processing begins, a reservation is released without consuming the input
when its worker, source, destination, or task becomes invalid. Stale tasks and
reservations are removed deterministically. Removal of a refinery after input
consumption is outside scope because building demolition is not supported.

Pausing the simulation stops travel, gathering, and processing. Changing
simulation speed changes the rate at which ticks occur but not any tick count,
recipe result, or ordering rule.

## Logistics and resource consumers

This feature expands resource acquisition beyond natural nodes. Within one
surface, eligible jobs may source resources from:

- Natural resource nodes.
- An NPC's already-carried inventory.
- Warehouse inventory.
- Farm inventory.
- Forester's Lodge inventory.
- Refinery input and output buffers. An external source search excludes the
  destination refinery itself; its own input buffer is checked directly first.
- Kitchen input and output buffers.

Source selection uses shortest reachable travel distance rather than straight
line distance. Unreachable sources are not eligible. Equal-distance sources are
ordered by source entity ID. Resource and capacity reservations are scoped to a
single surface and are included in availability checks.

The logistics behavior supports three consumers:

- **Refineries:** workers obtain a recipe input and deliver it to the appropriate
  refinery before processing.
- **Construction:** construction workers may obtain required materials from
  natural nodes or any eligible owned inventory, allowing Planks and Stone Blocks
  to reach blueprints. Existing batching and deposit behavior remains applicable.
- **Hunger:** hungry NPCs obtain cooked Food from Kitchen output buffers,
  Warehouses, or their own inventory. Crops and Wild Berries are never considered
  edible or valid hunger targets.

The Farm produces one Crop per successfully harvested Field instead of one Food.
The Forester's Lodge continues to receive Wood. Default NPCs retain their initial
20 Food so the settlement has time to establish a Kitchen.

There is no generic request to balance inventories or stock Warehouses. Resources
move only because an active refining, construction, or hunger need requests them.

## NPC eligibility, skills, and work priority

Add three independent eligibility tag components and three matching skills:

| Activity | Eligibility tag | Skill | XP event |
| --- | --- | --- | --- |
| Sawmill processing | Sawyer | Sawyer | +1 per completed Plank |
| Stoneworks processing | Stonemason | Stonemason | +1 per completed Stone Block |
| Kitchen processing | Cook | Cook | +1 per completed Food |

An NPC must have the matching tag to claim or perform the activity. Every default
NPC receives all three tags in addition to its current default eligibility. Skill
presence alone does not grant eligibility.

The new skills use the existing value, rank, percentage, clamping, and display
rules. Skill values do not alter processing duration, recipe yield, input cost,
or output quality in this feature. XP is granted only when output is successfully
completed, not for travel, hauling, partial progress, interruption, or a blocked
completion.

Work priority is:

1. Urgent personal hunger, which may interrupt any work.
2. Construction work.
3. Refining work.
4. Farming and forestry work.
5. Idle behavior.

Within refining work, an eligible NPC chooses the nearest reachable actionable
refinery. Equal distances are resolved by refinery entity ID. A refinery has at
most one maintained task and one worker claim. After a hunger interruption, a
still-valid refinery task remains available; the same worker is not guaranteed
to reclaim it if another eligible worker wins the deterministic assignment.

## User interface and player feedback

### Building palette

The palette order is:

1. Warehouse
2. Town Hall
3. Sawmill
4. Stoneworks
5. Kitchen
6. Farm
7. Forester's Lodge
8. Small House
9. Medium House
10. Large House

Buttons remain available at all times, matching current behavior. Placement uses
the existing valid/invalid footprint preview. There are no technology,
affordability, or production-state restrictions on entering placement mode.

### Selected building panel

The selected-building UI must support all resource kinds dynamically rather than
hard-coding the original four rows.

For a finished refinery it shows:

- Input and output buffer contents and capacities.
- Accepted input and produced output kinds, including accepted kinds at zero.
- Current recipe, or the automatic recipes supported while idle.
- Processing progress and remaining ticks.
- Assigned worker, or an unassigned state.
- At most one blocked reason.

When multiple blocked conditions apply, show the most immediately constraining
reason in this order: **Output full**, **No input**, then **No eligible worker**.
There are no recipe controls, production buttons, enable toggles, or target
quantities.

Construction progress shows every resource with a nonzero cost. Warehouse and
refinery inventories show accepted or stored resources, including zero-valued
accepted resources. Other building inventory views show resources relevant to
that building.

### NPC, resource, and task panels

- NPC inventory panels dynamically show only nonzero carried resources.
- NPC skill details include Sawyer, Stonemason, and Cook through the complete
  skill list.
- The global Resources panel remains one flat list of all eight resource kinds
  and tracks each kind in current totals, committed totals, and daily history.
- Tile and hover tooltips display Wild Berry nodes through the generic resource
  UI.
- Refining jobs appear in the Tasks panel. Each row exposes assignment state,
  worker when assigned, building, input-to-output recipe, and processing
  progress.

Bridge code must query surface-local simulation state and present it through
typed Godot methods, signals, and exported node references. It must not own
recipe, eligibility, reservation, or priority rules.

## Visual assets and animation

Add one imported resource icon/PNG for each new resource:

- Crops
- Wild Berries
- Planks
- Stone Blocks

Wild Berry natural nodes use the Wild Berries resource art. Crops exist as a
harvested inventory resource; existing Field growth-stage visuals remain in use.

Add one correctly sized imported building texture for each new footprint:

- Sawmill
- Stoneworks
- Kitchen

Blueprints reuse the finished-building texture with the existing blueprint tint;
separate blueprint images are not required.

Each refining activity receives a distinct four-frame, non-directional looping
NPC animation:

- `saw` while actively processing at a Sawmill.
- `stonecut` while actively processing at Stoneworks.
- `cook` while actively processing at a Kitchen.

Provide all three sheets for every current NPC appearance (five appearances,
fifteen new activity sheets in total), following the dimensions and frame layout
of the existing four-frame `gather` sheet. The activity animation is shown only
during the 60 processing ticks. Travel and hauling continue to use the normal
directional walking animations. Idle, blocked, reserved, and interrupted workers
do not play a refining animation.

All imported assets under `res://` are loaded as Godot resources through the
normal import pipeline. Missing exhaustive building or resource asset mappings
must be treated as validation failures rather than silently falling back.

## Architecture and data ownership

- `game_engine` owns resource definitions, recipes, buffers, production state,
  reservations, tasks, eligibility, assignment, source selection, timing, XP,
  hunger semantics, costs, and deterministic ordering.
- Every surface has independent production state, tasks, inventories,
  reservations, workers, and source searches. No job or query may cross surface
  boundaries.
- `godot_bridge` remains a thin typed adapter for placement commands, UI queries,
  task rows, render activity, and asset lookup.
- Godot scenes own layout and typed node wiring. No GDScript is introduced.
- Existing Godot enum values remain compatible; new variants and UI rows are
  additive.
- There is no persistence migration requirement in this feature, but future save
  support must treat refinery progress, consumed input, buffers, and reservations
  as durable simulation state where appropriate.

## Edge cases and failure behavior

- A refinery with no eligible input exposes no actionable batch and reports
  `No input` when selected.
- A refinery with full output storage does not reserve or consume new input and
  reports `Output full`.
- A refinery that could otherwise work but has no NPC with the matching tag
  reports `No eligible worker`.
- A source that becomes empty, unreachable, removed, or reserved by another job
  before pickup invalidates the claim; the worker releases it and may seek new
  work.
- An input carried when its destination becomes invalid remains in the NPC
  inventory when possible; no resource is deleted merely because a pre-processing
  job becomes stale.
- Hunger interrupts hauling or processing according to normal hunger priority.
  Pre-processing reservations are released; consumed input and refinery progress
  remain at the building.
- A missing worker, removed eligibility tag, or inaccessible interaction cell
  stops progress and releases the worker claim without resetting completed ticks.
- Duplicate tasks and claims for the same refinery or stock unit are removed
  deterministically.
- A Kitchen may switch between Crops and Wild Berries only between batches. An
  active batch retains the recipe chosen when its input was consumed.
- An empty colony leaves tasks unassigned and shows the applicable no-worker
  state; production never advances autonomously.
- All quantity and capacity updates remain overflow-safe and atomic.

## Acceptance criteria

### End-to-end behavior

- A new settlement can gather raw Wood and Stone, construct a Sawmill and
  Stoneworks, refine Planks and Stone Blocks, and use those outputs to construct
  every other building kind.
- A Farm harvest produces Crops, a Wild Berry node yields Wild Berries, and
  neither resource satisfies hunger directly.
- A Cook transports either ingredient to a Kitchen, completes 60 processing
  ticks, produces one Food, and gains one Cook XP.
- A hungry NPC can retrieve cooked Food from its own inventory, a Kitchen, or a
  Warehouse and consume it through the existing hunger model.
- Equivalent Sawmill and Stoneworks flows produce one refined unit and grant one
  matching skill XP.

### Simulation validation

- Exact recipe inputs, outputs, durations, buffer capacities, footprints, and all
  construction costs are covered by Rust tests.
- Natural generation produces only Wood, Stone, Wild Berries, and Gold while
  preserving deterministic surface generation.
- Only NPCs with the matching eligibility tag may claim each activity; default
  NPCs have all three new tags.
- Tests cover task creation/removal, deterministic worker/refinery/source choice,
  input and output reservations, competing workers, unreachable or disappearing
  sources, and duplicate cleanup.
- Tests cover interruption before and after input consumption, progress resumption
  by another worker, full-output blocking, unexpected capacity loss at
  completion, and no resource duplication or loss.
- Tests cover construction sourcing Planks and Stone Blocks from producer and
  Warehouse inventories, as well as raw bootstrap construction.
- Tests cover Kitchen selection of the nearest Crop or Wild Berry source without
  semantic ingredient preference.
- Tests cover Farm Crop output, cooked-Food-only hunger targeting, initial Food
  supplies, inventory capacity, overview totals, history, and stable Godot enum
  round-trips.
- At least one integration test proves that production, reservations, and source
  searches on one surface cannot observe or mutate another surface.

### Bridge, UI, and visual validation

- Palette buttons enter placement mode for all three new building kinds in the
  specified order.
- All eight resources appear correctly in the global Resources panel, tooltips,
  histories, and relevant dynamic inventory/construction rows.
- Selected refinery panels show buffers, recipes, progress, assignment, and the
  correct highest-priority blocked reason.
- Refining task rows show building, recipe, assignment, worker, and progress.
- Exhaustive resource and building asset mappings load all required imported
  textures.
- Every NPC appearance can play `saw`, `stonecut`, and `cook`; each animation has
  four frames, loops during active processing, and is not used during hauling or
  blocked states.
- Rust bridge tests cover view-data formatting and animation selection. A Godot
  headless startup validates scene wiring and imported asset availability without
  errors.

## Open decisions

None. This brief is decision-complete for implementation planning.
