# Boapspace Visual Art Bible

This document is the source of truth for authored and generated visual assets. The machine-readable companion is [`asset_manifest.toml`](asset_manifest.toml). An asset is not integration-ready until its manifest family has `ready = true` and the validation tests pass.

## Visual identity

Boapspace is a **lived-in sci-fi frontier** viewed from a consistent 3/4 top-down camera. The colony should feel assembled, repaired, and inhabited: sturdy silhouettes, practical shapes, patched surfaces, exposed fasteners, dusty wear, and small signs of repeated use. The style is painterly rather than photorealistic or pixel-art.

- Use broad readable value groups at gameplay zoom. Fine detail supports the silhouette; it never replaces it.
- Keep the world palette grounded in warm stone, weathered timber, oxidized metal, soil, moss, and muted vegetation.
- Use copper (`#C77B45`) for warmth, construction, and selected-state accents. Use cyan (`#54D7E8`) for advanced technology, holograms, and informational accents.
- Reserve saturated red and green for invalid/valid placement feedback. Avoid large cyan or copper fills that compete with the UI.
- Lighting is soft, fixed daytime illumination from the upper-left. Do not bake time-of-day colors into assets.
- Subjects use a consistent 3/4 top-down perspective. Vertical elements remain legible; roofs must not obscure the footprint or interaction point.

## Scale and camera contract

The simulation grid remains 64 Godot units per logical tile. World raster art is authored at **256 source pixels per logical tile or animation frame** and rendered at **0.25 scale**. Do not resize the simulation, collision, navigation, or input coordinates to match source pixels.

- Terrain, farming overlays, road cells, character frames, and vehicle frames are 256×256 source pixels.
- Buildings use a canvas sized to their logical footprint: a 2×2 building has a 512×512 source canvas, for example.
- Character and vehicle sheets use four columns. Eight-direction movement sheets use eight rows ordered by the consuming scene/code contract.
- Interface icons remain project-native SVG where possible. Raster UI illustrations are authored for their explicit layout, not downscaled from world sprites.
- World textures use linear filtering, mipmaps, and alpha-border fixing. Large atlases should use Godot's VRAM-compressed import mode; UI textures stay lossless.

## Composition, anchors, and transparency

The manifest stores normalized anchors relative to each full frame or building canvas.

- Terrain, road cells, and tile overlays anchor at center `(0.5, 0.5)` and must fill their tile without transparent seams unless the asset is explicitly an overlay.
- Buildings anchor at the bottom-center `(0.5, 1.0)`. Their contact point and entrance belong near the lower edge, with all pixels contained inside the footprint-sized canvas.
- Characters anchor between their feet at `(0.5, 0.875)`. Keep head height, body mass, and feet position stable across every frame and activity.
- Wheelbarrows anchor at ground contact `(0.5, 0.875)`. Loaded variants must edit basket contents only; chassis, perspective, silhouette, and frame registration match the empty master.
- Cutout assets require a real alpha channel, transparent background, clean color edges, and at least one transparent pixel. Never bake contact shadows, smoke, dust, or glow into a cutout.
- Contact shadows and translucent activity effects are separate assets/nodes so they can be tuned and culled independently.

## Asset families

- **Terrain:** four deterministic variants per source sheet. Every 256×256 cell must tile seamlessly on all sides. Water uses four restrained animation frames without camera-relative sparkle or shoreline baked in. Terrain edge masks, when added, are separate 16-mask atlases.
- **Roads:** 16 connectivity masks in a 4×4 atlas, ordered by the four-neighbor bitmask used by rendering. Each cell remains visually centered and joins its neighbors at consistent widths.
- **Buildings:** one transparent, footprint-sized canvas per kind. Construction holograms/scaffolding and activity effects are separate; do not alter final-building art for construction progress.
- **Characters:** lock one reference per appearance. Idle and work activities are four-frame horizontal strips; walk is a four-column by eight-direction grid. Maintain identity, equipment, lighting, and proportions across sheets.
- **Resources and farming:** world resources are readable natural piles or objects, not interface glyphs. Crop/tree stages keep their ground contact fixed and progress visibly without changing camera angle.
- **Vehicles:** author the empty wheelbarrow first, then derive every load by editing basket contents only.
- **UI:** use Oxanium for headings and Inter for body copy when the bundled fonts are present. Use copper/cyan accents, painted nine-slice panels, crisp SVG controls, clear focus/disabled/error states, and readable translucent surfaces.
- **Menu:** compose the vista from an opaque 4K background, a colony midground, and a transparent foreground. Motion is a seamless restrained loop with only subtle parallax and atmospheric drift.

## Generation and review workflow

1. Establish and approve one shared style reference before generating a family.
2. Generate each distinct asset or sheet from that reference. Use constrained edits for variants that must retain identity.
3. Extract cutouts against a flat chroma background; regenerate extraction failures rather than accepting halos or clipped silhouettes.
4. Place the PNG at the manifest path and inspect it at 0.25 scale as well as source resolution.
5. Check exact dimensions, frame registration, alpha edges, anchors, terrain seams, and animation continuity.
6. Set the family to `ready = true`, then run `cargo test --manifest-path rust/Cargo.toml --test visual_asset_manifest`.

Do not set `ready = true` as a progress marker. It means every file in the family is present at its final path and conforms to the complete contract.
