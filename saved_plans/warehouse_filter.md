# Warehouse filtering and NPC resource carrying

## Summary

Replace generic NPC inventories with a personal food pouch and a separate work-cargo container. Add per-resource warehouse deposit filters and move building information and controls from left-click selection into a right-click context panel.

## Goals

- Configure which resources each warehouse accepts.
- Give NPCs a personal `FoodPouch` and a separate `CarriedResource`.
- Limit work cargo to five units of one resource kind.
- Visually identify carried resources using existing resource icons.
- Provide a general right-click information and control panel for buildings.

## Non-goals

- Autonomous stockpiling or general hauling into warehouses.
- Save/load support or migration from the old NPC inventory.
- Filtering warehouse withdrawals.
- New baked carrying-animation sheets or quantity-specific cargo visuals.

## Building context panel

- Buildings are no longer part of left-click selection and the old left-menu building panel is removed. Left-click continues to select tiles and NPCs.
- Right-clicking any completed building or building blueprint opens a cursor-anchored context panel and highlights the target footprint.
- The panel contains all existing building information and controls: name, footprint, construction, inventory, farming, forestry, refinery, housing, Fields, and Tree Plots.
- A warehouse blueprint gains filter controls when it completes while the panel is open.
- The panel closes on Escape, an outside click, surface switch, target deletion, or starting plot placement. Outside clicks are consumed.
- While placement mode is active, right-click cancels placement instead of opening a panel.

## Warehouse filtering

- Each resource has an immediate `allowed` checkbox. New warehouses allow every resource, and allowing none is valid.
- Future resource kinds default to allowed.
- Filters affect future deposits only. Already-stored disallowed stock remains visible and withdrawable, and outbound reservations stay valid.
- Rejected changes are atomic: the UI refreshes from simulation state or closes if the target is no longer a completed warehouse.
- This feature enforces the deposit contract but does not add autonomous warehouse-filling jobs.

## NPC containers and business rules

### Food pouch

- Contains cooked `Food` only.
- Capacity is 100; NPCs start with 20.
- Refill starts at five or less and targets 100.
- Hunger consumes only from the pouch.

### Carried resource

- Capacity is five and contents are empty or one `ResourceKind`.
- Food may be work cargo independently from personal Food.
- Adds are atomic: different kinds, overflow, or insufficient capacity reject the entire request.
- The carried kind clears when its quantity reaches zero.
- Cancelled or preempted work leaves cargo on the NPC. Compatible work is preferred later; otherwise cargo remains without preventing work that requires no cargo.

Owned-source construction batches are capped at five. Existing one-unit natural gathering and refining transfers remain one unit. Hunger can preempt work without discarding cargo.

## UI and visuals

- NPC summary, details, and tooltips show `Food Pouch: current/100` and `Carried Resource: kind/current/5`, including an explicit empty cargo state.
- Colony totals and history count carried cargo exactly once and exclude personal Food Pouches.
- The existing icon for the carried kind appears as an NPC overlay whenever cargo is non-empty, including while idle, moving, gathering, or refining.

## Architecture and persistence

- Filters, container invariants, and logistics rules live in `game_engine`; the Godot bridge exposes typed view data and commands.
- Filter commands are surface-scoped and accept only completed warehouses.
- State remains isolated per surface. Save persistence is not currently applicable.
- Godot integration uses typed signals, calls, exported references, and `ResourceLoader` assets.

## Acceptance criteria

- Filters default to allow-all, permit allow-none, reject disallowed deposits without mutation, and preserve withdrawals of existing stock.
- Filter commands reject blueprints, non-warehouses, missing entities, and wrong-surface entities.
- Food Pouches refill from five to 100 and are independent of five-unit cargo, including when both contain Food.
- Every logistics path respects cargo capacity and never silently loses retained cargo.
- Totals include cargo and exclude personal food.
- Left-click never selects buildings; right-click resolves every footprint cell and observes placement-cancellation priority.
- The context panel mirrors the former building panel, refreshes live, and closes safely for stale targets and surface changes.
- Every NPC information surface and appearance supports the carried-resource icon overlay.
