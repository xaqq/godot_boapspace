# Deterministic 3D asset pipeline

The experimental renderer's shipping GLBs and shared texture atlases are built
from declarative recipes, not from tracked `.blend` files. Blender 5.0.1 is the
only supported generator version.

The Blender glTF add-on imports NumPy. On Ubuntu, install the matching system
package before generation:

```bash
sudo apt-get install python3-numpy
```

The two approved image-generation results are immutable inputs. Their exact
SHA-256 values and generation prompts are recorded in
`art_sources/world_3d/source_manifest.toml`. The generator fails closed if a
source is absent, unapproved, or has changed.

## Generate and validate

Run commands from the repository root:

```bash
blender --background --factory-startup \
  --python tools/assets_3d/generate.py

blender --background --factory-startup \
  --python tools/assets_3d/generate.py -- --check

blender --background --factory-startup \
  --python tools/assets_3d/validate.py -- \
  --manifest godot/assets/visual/asset_manifest_3d.toml

cargo test --manifest-path rust/Cargo.toml \
  --test visual_asset_manifest_3d
```

`--check` regenerates all 35 GLBs and 15 atlas outputs into a temporary
repository-shaped directory and byte-compares them with shipping files. The
post-export canonicalization preserves triangle winding while sorting opaque
triangle draw order, removing Blender BMesh's otherwise nondeterministic index
ordering. GLBs reference the five external shared atlas sets, so Godot does not
extract per-model texture duplicates.

After changing generated output, refresh and commit the Godot 4.7 `.import`
metadata:

```bash
~/.local/bin/godot4 --headless --editor --path godot --quit-after 30
~/.local/bin/godot4 --headless --path godot \
  res://world/3d/asset_smoke_test.tscn --quit-after 2
~/.local/bin/godot4 --headless --path godot \
  res://world/3d/world_renderer_smoke_test.tscn
```

Temporary `.blend` files, turntables, and `.godot/imported` cache files are not
pipeline inputs and must remain untracked.
