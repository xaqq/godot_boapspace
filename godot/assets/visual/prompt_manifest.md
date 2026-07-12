# Visual generation manifest

All newly generated raster artwork uses the built-in image generation workflow and
the 4K menu vista as the shared style reference. Final cutouts are generated on a
flat `#00ff00` background, extracted with the installed soft-matte/despill helper,
then fitted to the exact canvas declared in `asset_manifest.toml`.

## Shared style contract

- Use case: `stylized-concept`.
- Style: high-resolution painterly 2D, lived-in sci-fi frontier.
- Camera: elevated 3/4 top-down orthographic for world objects; straight top-down
  for terrain materials.
- Palette: weathered white metal, warm copper repairs, restrained cyan technology,
  organic muted terrain.
- Lighting: fixed soft daylight from the upper-left.
- Constraints: readable silhouette at gameplay zoom; no text, logos, watermark,
  baked shadow, smoke, or unrelated props.

## Opaque generated artwork

| Output | Prompt subject |
| --- | --- |
| `menu/frontier_vista.png` | Wide frontier colony vista with title and menu breathing room |
| `world/terrain/terrain_grass.png` | Seamless mossy frontier grass and sparse alien ground cover |
| `world/terrain/terrain_dirt.png` | Seamless compacted soil with restrained wheel scuffs and minerals |
| `world/terrain/terrain_sand.png` | Seamless pale alien sand with subtle granular variation |
| `world/terrain/terrain_water.png` | Seamless tranquil turquoise shallow water |
| road atlas surfaces | Seamless cobblestone and flagstone materials, clipped through the connectivity masks |

Each terrain output is normalized to one 256 px tile, then expanded to four
deterministic variants using wrapped offsets and restrained color modulation.

## Chroma-key generated cutouts

| Family | Generated subjects |
| --- | --- |
| Buildings | Depot, warehouse, town hall, sawmill, stoneworks, kitchen, farm, forester lodge, small/medium/large houses |
| Resources | Wood, stone, prepared food, gold ore, harvested crops, wild berries, planks, stone blocks |
| Farming | Empty plot, two growing crop stages, mature crop, sapling, young tree plot, mature tree plot |

Building prompts require the exact gameplay footprint, bottom-center grounding,
and no loose scenery. Resource and farming prompts require a compact tile-readable
silhouette. The generated subject never contains the chroma-key color.

## Compatibility-authored sheets

Directional character and wheelbarrow sheets retain their existing animation and
identity contract, are re-authored at 256 px per frame through high-quality
resampling, and use the same linear-mipmap rendering path. This avoids introducing
frame-to-frame identity drift while the new manifest, scaling, filtering, depth,
and shadow systems provide the 4K-ready presentation contract.

