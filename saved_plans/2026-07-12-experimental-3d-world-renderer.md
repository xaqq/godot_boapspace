# Experimental Fully Interactive 3D World Renderer

## Summary

Add a second renderer for the existing simulation, selectable through `2D` and
`3D` header buttons.

- Start every game in 2D; do not persist renderer choice.
- Preserve the active surface, selection, construction tool, and camera focus
  when switching. Cancel only an in-progress road/plot drag.
- Retain independent 2D zoom and 3D zoom/orbit settings.
- Provide complete interaction parity in 3D: hover, selection, context panels,
  construction, previews, routes, and surface switching.
- Keep terrain logically flat and make no gameplay, physics, or `game_engine`
  changes.
- Target sustained 60 FPS at 1440×900 on a typical discrete-GPU desktop.

## Controller, UI, and shared rendering data

1. Refactor `GameWorld` from `Node2D` to `Node`, retaining it as the sole owner
   of `GameSimulation`, ticking, surfaces, interaction state, commands,
   signals, and performance sampling.
2. Add typed child renderers under the existing `GameWorld` scene path:

   ```text
   GameWorld
   ├── Renderer2D : WorldRenderer2D/Node2D
   └── Renderer3D : WorldRenderer3D/Node3D
   ```

   Move the current tile maps, sprites, drawing, and `Camera2D` into
   `WorldRenderer2D`. Existing panels keep their current typed `Gd<GameWorld>`
   references unchanged.

3. Extract deterministically sorted renderer-neutral data:

   - `SurfaceRenderSnapshot`: surface ID, dimensions, terrain kind/variant per
     cell.
   - `DynamicRenderSnapshot`: roads, buildings/blueprints, resources, crop/tree
     stages, NPC activity/facing/cargo/wheelbarrows.
   - `WorldOverlaySnapshot`: selections, route, building preview, and
     valid/invalid road/plot cells.
   - `NpcActivity`: `Idle`, `Walk`, `Gather`, `Saw`, `Stonecut`, `Cook`.

4. Add bridge-only APIs:

   ```rust
   enum RendererMode { TwoD = 0, ThreeD = 1 }

   GameWorld::active_renderer_mode()
   GameWorld::renderer_mode_available(mode)
   GameWorld::set_renderer_mode(mode) -> bool
   GameWorld::renderer_mode_changed(mode)
   ```

5. Build one dynamic snapshot after the single simulation tick and update only
   the active renderer. Keep the latest snapshots cached so a stale renderer
   receives a full sync before activation.
6. Add `Preparing`, `Ready`, and `Failed` 3D availability states. Prewarm
   terrain/assets incrementally while 2D remains active; disable the 3D button
   with an explanatory tooltip until ready. On failure, remain in or restore 2D
   without stopping the simulation.
7. Add `2D` and `3D` buttons after the speed controls. Disable the active
   button, matching the speed-button convention; label the 3D tooltip as
   experimental.

## 3D renderer and interaction

- Use two 3D units per logical tile: simulation `(x, y)` maps to `(X, Z)`, with
  `Y` as height. Subtile offsets map proportionally from 1024 units per tile.
- Render terrain and roads as 32×32-tile `ArrayMesh` chunks. Terrain rebuilds
  only for a new surface; road changes rebuild only affected chunks. Reuse the
  existing terrain variants and road atlases.
- Load the custom GLB meshes through `ResourceLoader`. Batch static buildings,
  resources, and farming stages by chunk, model, render state, and LOD using
  `MultiMesh`; instantiate NPC skeleton scenes individually.
- Reuse completed building meshes for blueprints and previews with cyan,
  valid-green, or invalid-red material overrides. Reuse resource meshes for
  carried cargo and wheelbarrow loads.
- Render grid, footprint selection, placement cells, and NPC routes as unshaded
  3D ribbons/meshes above the ground. Preserve existing colors, chevrons, and
  blocked-route symbols.
- Use CPU pick proxies derived from model bounds rather than physics bodies.
  Camera rays first test chunk-indexed building/NPC/resource AABBs, then
  intersect the flat ground plane. Revalidate all resulting cells/entities
  against the ECS world and retain existing selection priority.
- Construction and drag placement always use the ground-plane cell, ignoring
  model geometry. Right-click building context uses the building proxy first to
  avoid roof/perspective ambiguity.
- Centralize input in `GameWorld`:

  - WASD pans relative to the active camera.
  - Middle-mouse drag orbits 3D and captures input until release.
  - Wheel zooms 2D or dollies 3D.
  - Left/right map behavior remains unchanged.
  - Hover is suppressed during orbit.

- Configure the perspective camera with 50° FOV, initial yaw/elevation of
  45°/55°, pitch clamped to 25°–80°, initial distance of 24 tiles, and distance
  clamped between 4 tiles and 1.25 times the surface diagonal. Transfer only
  tile-space focus when switching renderers.
- Use one upper-left `DirectionalLight3D`, ambient sky lighting, ACES
  tonemapping, and restrained shadows. Initially disable GI, SSAO, fog, and
  glow. Cull detailed buildings/trees after roughly 64 tiles,
  resources/farming after 48, and animated NPCs/tools/vehicles after 32,
  retaining lightweight overview proxies.

## Generated 3D asset pipeline

- Pin regeneration to the verified Blender 5.0.1 CLI. Use tracked Python
  recipes under `tools/assets_3d/`, immutable image-generation sources under
  `art_sources/world_3d/`, and shipping assets under
  `godot/assets/visual/world/3d/`.
- Use built-in image generation for opaque model-reference turnarounds and
  painterly material sources, using the existing matching 2D assets and
  frontier vista as references. Record prompts, inputs, selected outputs,
  roles, and hashes.
- Generate shared structure, timber, masonry/ore, organic, and character
  material atlases. Derive normal and ORM maps mechanically; reuse current
  terrain/road artwork. Do not bake directional lighting into textures.
- Generate and commit GLBs for:

  - All 13 building kinds.
  - All 8 resource kinds.
  - Four crop and three tree stages.
  - Five NPC appearances.
  - Wheelbarrow and work-prop libraries.

- Use one canonical humanoid rig recipe to export five self-contained character
  GLBs. Required in-place 30 FPS clips are `idle`, `walk`, `gather`, `saw`,
  `stonecut`, `cook`, `carry_idle`, `carry_walk`, `wheelbarrow_idle`, and
  `wheelbarrow_walk`; wheelbarrows expose `idle` and `roll`.
- Generate typed Godot wrapper scenes for animation players and
  hand/carry/wheelbarrow bone attachments. No runtime string-path node lookup
  and no billboard fallback.
- Commit generator sources, approved reference/material PNGs, final GLBs, Godot
  wrappers/materials, and stable `.import` metadata. Keep temporary `.blend`
  files and turntable renders untracked.
- Add a separate `asset_manifest_3d.toml` describing Blender/Godot versions,
  scale/orientation, material outputs, every model path, footprint, material
  slots, triangle budget, skeleton/animation contract, provenance hash, and
  readiness. Keep the existing raster manifest unchanged.
- Use binary glTF because Godot 4.7 recommends glTF 2.0/GLB for meshes,
  materials, skeletons, and animations; generation runs through Blender's
  documented background/Python interface. [Godot 4.7 import
  formats](https://docs.godotengine.org/en/4.7/tutorials/assets_pipeline/importing_3d_scenes/available_formats.html),
  [Blender command-line
  interface](https://docs.blender.org/manual/en/4.5/advanced/command_line/arguments.html)

## Tests and acceptance

- Preserve all existing 2D behavior tests after extracting `WorldRenderer2D`.
- Add unit tests for snapshot completeness/order, NPC activity precedence, road
  masks, chunk boundaries, coordinate conversion, camera clamps,
  ray/ground/AABB picking, dirty-chunk diffs, mode-transition ordering,
  unavailable/failure fallback, and header button state.
- Add GLB/manifest tests covering every enum variant, hashes, triangle budgets,
  materials, pivots, skins, shared bone hierarchy, exact animation names,
  stationary roots, and valid weights.
- Add a headless asset-gallery scene that instances every model/wrapper and
  exercises every animation.
- Validate in this order:

  ```bash
  cargo build --manifest-path rust/Cargo.toml
  cargo test --manifest-path rust/Cargo.toml
  blender --background --factory-startup \
    --python tools/assets_3d/validate.py -- \
    --manifest godot/assets/visual/asset_manifest_3d.toml
  ~/.local/bin/godot4 --headless --editor --path godot --quit-after 2
  ~/.local/bin/godot4 --headless --path godot \
    res://world/3d/asset_smoke_test.tscn --quit-after 2
  ~/.local/bin/godot4 --headless --path godot \
    res://world/game_world.tscn --quit-after 2
  ```

- Runtime acceptance requires repeated 2D/3D switching at 1× and 100× without
  duplicate ticks or entity resets; preserved selection/tool/focus; every
  selection and construction flow working in 3D; correct surface invalidation;
  visible models for every current asset family and state; and sustained 60 FPS
  after shader warmup on the reference desktop.

## Risks and edge cases

- A renderer split can regress mature 2D behavior. Complete the 2D extraction
  and run its existing tests before exposing the 3D mode.
- Image generation is nondeterministic. Treat the approved committed source
  images as immutable inputs and make Blender output reproducible from those
  inputs, the recipes, and the pinned Blender version.
- Blender/Godot import naming or material remapping can drift. Prove one
  representative building, NPC rig, resource, crop/tree stage, and wheelbarrow
  through the full import and smoke-test path before generating the full set.
- Orbiting perspective creates roof-versus-ground ambiguity. The CPU model
  proxies and ECS revalidation must remain separate from construction's
  ground-plane projection.
- Surface switching during incremental 3D preparation must cancel stale work so
  chunks from the previous surface cannot appear.
- Do not introduce Git LFS or a new runtime dependency on Blender. Shipping
  GLBs and textures remain committed project assets.

## Assumptions and non-goals

- Blender 5.0.1 and Godot 4.7 are available in the development environment.
- The current `game_engine` simulation and its 64-unit 2D logical coordinate
  contract remain unchanged; the 2-unit 3D scale is bridge-only.
- The 2D renderer remains the startup default and authoritative fallback.
- Renderer preference is not saved across launches.
- Terrain elevation, 3D gameplay physics, new simulation rules, weather,
  time-of-day systems, and replacement/removal of existing 2D artwork are out
  of scope.
- No GDScript is introduced.
- There are no open implementation decisions.
