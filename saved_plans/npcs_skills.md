# NPC Skills

## Summary

Add persistent skills to NPCs. A skill is an NPC-specific numeric experience
value in a named field, displayed to the player as a percentage, progress bar,
and textual rank.

For this feature, skills are informational only. They do not affect work speed,
task assignment, resource yield, movement, hunger, construction progress, or any
other simulation outcome.

## Goals

- Add a skills model to NPC simulation data in `game_engine`.
- Track skill values from `0` to `10000`.
- Award gathering skill experience when an NPC successfully gathers a resource
  unit.
- Display all current selected-NPC information plus skills in a dedicated NPC
  details panel.
- Keep durable rules in the Rust simulation and keep the Godot bridge/UI thin.

## Non-Goals

- No skill effects on gameplay.
- No profession assignment, priorities, task filters, work restrictions, or AI
  changes based on skill.
- No Builder XP yet. Builder exists as a visible skill but remains at `0` until
  a future dedicated building-work action is designed.
- No Farmer XP yet. Farmer is reserved for future agriculture/farming mechanics.
  Current food resource gathering uses Forager instead.
- No save/load migration work, because the project does not currently have a
  save/load system.

## Skill List

The initial skill set is fixed and shown in this order:

1. Builder
2. Farmer
3. Lumberjack
4. Quarryman
5. Forager
6. Prospector

Resource gathering skills map to existing resource kinds:

- Wood -> Lumberjack
- Stone -> Quarryman
- Food -> Forager
- Gold -> Prospector

Builder and Farmer are visible placeholder skills for future mechanics. They
start at `0`, can show `Untrained`, and are not modified by current systems.

## Skill Values

Each skill value is an integer in the inclusive range `0..=10000`.

- `0` means the NPC has never successfully performed that skill's activity.
- `10000` is the maximum value.
- Any XP gain that would exceed `10000` saturates at `10000`.
- New NPCs start with all skills at `0`.
- NPCs created by tests or custom setup should have an explicit skills
  component. UI and bridge code should still handle a missing skills component
  defensively by treating it as all zeroes rather than crashing.

## Textual Ranks

Ranks are derived directly from the numeric skill value.

| Value Range | Rank |
| --- | --- |
| `0` | `Untrained` |
| `1..=999` | `Novice` |
| `1000..=2499` | `Apprentice` |
| `2500..=4999` | `Journeyman` |
| `5000..=7499` | `Skilled` |
| `7500..=9499` | `Expert` |
| `9500..=9999` | `Master` |
| `10000` | `GrandMaster` |

## XP Rules

Only successful resource gathering awards XP in this feature.

- When `system_gather_resource` successfully transfers one resource unit from a
  resource node into an NPC inventory, the NPC gains `+1` in the matching
  resource gathering skill.
- The award happens after the inventory add succeeds and the gathered resource
  kind is known.
- The reason for gathering does not matter. Food gathered for hunger refill,
  wood or stone gathered for construction, and any other successful resource
  gathering all award the resource-specific skill.
- Failed, interrupted, invalid-target, moved-away, depleted-node, or
  full-inventory gathering attempts award no XP.
- Partial progress toward a gather action awards no XP.
- Depositing resources into construction awards no XP.
- Completing a building awards no XP.
- Hunger consumption, inventory transfers, and idle movement award no XP.

## Simulation And ECS Requirements

Skills are durable NPC simulation state and belong in `game_engine`, not in the
Godot bridge.

The simulation model should provide:

- A typed skill kind representation for the six skills listed above.
- Skill metadata for labels, rank calculation, maximum value, and percent
  calculation.
- An NPC skills component that stores the fixed skill set.
- An API on the skills component to read a skill value and add XP with
  saturation at `10000`.
- A deterministic mapping from `ResourceKind` to the matching resource gathering
  skill.

The default NPC bundle should include the skills component. Skills are scoped to
the NPC entity and therefore to the surface `World` that owns that NPC. Skills
do not cross surfaces.

## UI Behavior

The existing selected NPC panel remains compact. It gets a `Details` button.

- The button is disabled when no NPC is selected.
- Pressing the button opens the NPC details panel.
- The NPC details panel behaves like the existing Tasks panel: it is an overlay
  under the main map viewport area, hidden by default, with a close button.
- The panel follows the currently selected NPC live. If the selected NPC changes
  while the panel is open, the panel updates to the new NPC.
- If selection is cleared while the panel is open, the panel stays open and
  shows an empty state.
- If the selected NPC entity disappears, the panel treats it the same as cleared
  selection.

The NPC details panel shows all information currently available in the selected
NPC panel:

- Name
- Age
- Birth day
- Cell position
- Hunger state and satiation progress
- Inventory

It also shows a Skills section containing every skill, including zero-value
skills. Each skill row shows:

- Skill name.
- Percentage value, rounded to the nearest whole number from
  `skill_value / 10000 * 100`.
- A progress bar from `0` to `100`.
- Textual rank.

The UI may include the raw `0..10000` value in a tooltip, but the row itself
should remain readable without requiring tooltips.

## Godot Bridge Requirements

Godot should query read-only NPC details from the rendered surface world. The
bridge should expose or share a typed Rust DTO for NPC details rather than
duplicating extraction logic in multiple panels.

Expected bridge/UI shape:

- Reuse `GameWorld` NPC selection signals: `npc_selected(i64)` and
  `npc_deselected()`.
- Keep entity IDs encoded through the existing Bevy entity ID helpers.
- Add a Rust `NpcDetailsPanel` class and matching
  `godot/panel/npc_details_panel.tscn`.
- Register the new panel module in `rust/godot_bridge/src/lib.rs`.
- Add an exported typed `Button` reference to the selected NPC panel for opening
  details.
- Use exported typed `OnEditor<Gd<T>>` references for scene nodes.
- Do not use GDScript.
- Avoid stringly `Gd::call` access except where Godot APIs require strings.

## Data And Persistence

There is no current save/load system, so no save migration is required for this
feature.

When save/load exists, NPC skills must be persisted with the rest of durable NPC
state. Skill data should be stable enough to survive future skill additions by
using typed skill identifiers and defaulting unknown or newly-added skills to
`0` when appropriate.

## Edge Cases

- No NPC selected: selected NPC panel disables the Details button; details panel
  shows an empty state if already open.
- NPC selected on a different surface: details only show data from the rendered
  surface. Surface switching clears or refreshes selection according to existing
  selection behavior.
- NPC entity removed while details panel is open: panel shows empty state and
  does not panic.
- NPC lacks skills component: UI treats all skills as `0`; simulation systems
  should not panic.
- Gathering target missing or no longer a resource node: no XP.
- NPC is not on the target resource tile: no XP.
- Resource node quantity is zero: no XP.
- NPC inventory is full or cannot accept the gathered unit: no XP.
- Gathering progress is below the required tick count: no XP.
- Skill is already at `10000`: successful matching actions leave it at
  `10000`.
- Paused simulation: no ticks run, so no skill progress changes.
- Increased simulation speed: XP awards remain per successful simulated gather
  unit, not per rendered frame.

## Acceptance Criteria

- The initial default NPC has all six skills at `0`.
- Every NPC spawned through the standard NPC bundle has a skills component.
- `SkillKind` labels and `SkillRank` thresholds match this document.
- Skill percentages round to the nearest whole number and clamp to `0..=100`.
- Successful gathering of Wood increments Lumberjack by `1`.
- Successful gathering of Stone increments Quarryman by `1`.
- Successful gathering of Food increments Forager by `1`.
- Successful gathering of Gold increments Prospector by `1`.
- Builder and Farmer remain at `0` after gathering, construction resource
  deposit, and building completion.
- Failed and interrupted gather attempts do not change any skill.
- Skill values never exceed `10000`.
- The selected NPC panel exposes a disabled/enabled Details button based on NPC
  selection.
- The NPC details overlay opens from the Details button, closes from its close
  button, and refreshes live while visible.
- The NPC details panel shows existing selected-NPC information plus every skill
  row with percentage, progress bar, and rank.
- Clearing selection while details are open shows an empty state without closing
  the panel.

## Tests And Validation

Simulation tests should cover:

- Initial NPC skills are present and zeroed.
- Rank calculation at every threshold boundary.
- Percent calculation and rounding.
- XP saturation at `10000`.
- Successful gather awards the correct resource gathering skill.
- Failed gather paths do not award XP.
- Construction resource deposit and building completion do not award Builder XP.
- Farmer remains unchanged by current systems.

Bridge/UI tests should cover pure formatting and DTO behavior where practical:

- NPC details DTO includes existing NPC fields and all skills.
- Missing skills component is represented as zeroes.
- Skill row formatting returns the expected percent and rank.
- Details button state follows selected/deselected NPC state.

Manual Godot validation should confirm:

- The selected NPC panel remains compact.
- The Details button appears in the selected NPC panel.
- The details overlay matches the Tasks overlay pattern.
- The skills section is readable and updates after successful gathering.

## Relevant Existing Code

- NPC marker/data components: `rust/game_engine/src/components.rs`
- Default NPC bundle and hunger update: `rust/game_engine/src/npcs.rs`
- Resource kinds and resource amount storage pattern:
  `rust/game_engine/src/resources.rs`
- Gathering and construction AI systems: `rust/game_engine/src/ai.rs`
- Surface schedule: `rust/game_engine/src/systems.rs`
- Multi-surface simulation ownership: `rust/game_engine/src/simulation.rs`
- Existing selected NPC panel:
  `rust/godot_bridge/src/panel/npc_info_panel.rs`
- Existing Task overlay pattern:
  `rust/godot_bridge/src/panel/task_list_panel.rs` and
  `godot/panel/task_list_panel.tscn`
- Main scene panel placement: `godot/world/game_world.tscn`

## Open Decisions

None for this feature scope.

Future feature work must decide how Builder and Farmer gain XP once dedicated
building-work and agriculture/farming mechanics exist.
