"""Validate the complete generated 3D asset contract under Blender 5.0.1.

The validator reads GLB containers directly for deterministic structural checks;
it does not trust Blender's current scene or generated ``.import`` metadata.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
import sys
import tomllib
from pathlib import Path
from typing import Any, Sequence

import bpy

SCRIPT_DIR = Path(__file__).resolve().parent
REPOSITORY_ROOT = SCRIPT_DIR.parents[1]
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import recipes  # noqa: E402


COMPONENT_FORMATS = {
    5120: ("b", 1),
    5121: ("B", 1),
    5122: ("h", 2),
    5123: ("H", 2),
    5125: ("I", 4),
    5126: ("f", 4),
}
TYPE_COMPONENTS = {
    "SCALAR": 1,
    "VEC2": 2,
    "VEC3": 3,
    "VEC4": 4,
    "MAT2": 4,
    "MAT3": 9,
    "MAT4": 16,
}


def parse_arguments() -> argparse.Namespace:
    blender_args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=recipes.ASSET_MANIFEST_PATH,
        help="repository-relative or absolute generated manifest path",
    )
    parser.add_argument("--root", type=Path, default=REPOSITORY_ROOT)
    parser.add_argument("--skip-version-check", action="store_true")
    return parser.parse_args(blender_args)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_glb(path: Path) -> tuple[dict[str, Any], bytes]:
    data = path.read_bytes()
    require(len(data) >= 20, f"{path} is too short to be a GLB")
    magic, version, declared_length = struct.unpack_from("<4sII", data, 0)
    require(magic == b"glTF", f"{path} has an invalid GLB magic")
    require(version == 2, f"{path} is glTF {version}, expected 2")
    require(declared_length == len(data), f"{path} has an invalid declared length")

    cursor = 12
    json_document: dict[str, Any] | None = None
    binary = b""
    while cursor < len(data):
        require(cursor + 8 <= len(data), f"{path} has a truncated chunk header")
        length, chunk_type = struct.unpack_from("<II", data, cursor)
        cursor += 8
        chunk = data[cursor : cursor + length]
        require(len(chunk) == length, f"{path} has a truncated chunk")
        cursor += length
        if chunk_type == 0x4E4F534A:
            json_document = json.loads(chunk.rstrip(b" \t\r\n\0").decode("utf-8"))
        elif chunk_type == 0x004E4942:
            binary = chunk
    require(json_document is not None, f"{path} has no JSON chunk")
    return json_document, binary


def normalized_component(value: int | float, component_type: int, normalized: bool) -> float:
    if not normalized or component_type == 5126:
        return float(value)
    if component_type == 5120:
        return max(float(value) / 127.0, -1.0)
    if component_type == 5121:
        return float(value) / 255.0
    if component_type == 5122:
        return max(float(value) / 32767.0, -1.0)
    if component_type == 5123:
        return float(value) / 65535.0
    if component_type == 5125:
        return float(value) / 4294967295.0
    raise RuntimeError(f"unsupported normalized component type {component_type}")


def accessor_values(
    document: dict[str, Any], binary: bytes, accessor_index: int
) -> list[tuple[float, ...]]:
    accessor = document["accessors"][accessor_index]
    require("sparse" not in accessor, "sparse accessors are intentionally unsupported")
    view = document["bufferViews"][accessor["bufferView"]]
    component_type = int(accessor["componentType"])
    require(component_type in COMPONENT_FORMATS, f"unsupported component type {component_type}")
    format_code, component_size = COMPONENT_FORMATS[component_type]
    components = TYPE_COMPONENTS[accessor["type"]]
    packed_size = component_size * components
    stride = int(view.get("byteStride", packed_size))
    require(stride >= packed_size, "accessor stride is smaller than its packed value")
    offset = int(view.get("byteOffset", 0)) + int(accessor.get("byteOffset", 0))
    values: list[tuple[float, ...]] = []
    for index in range(int(accessor["count"])):
        start = offset + index * stride
        require(start + packed_size <= len(binary), "accessor exceeds GLB binary chunk")
        unpacked = struct.unpack_from("<" + format_code * components, binary, start)
        values.append(
            tuple(
                normalized_component(value, component_type, bool(accessor.get("normalized")))
                for value in unpacked
            )
        )
    return values


def primitive_triangle_count(document: dict[str, Any]) -> int:
    triangles = 0
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            mode = int(primitive.get("mode", 4))
            require(mode == 4, "all generated primitives must use TRIANGLES mode")
            if "indices" in primitive:
                count = int(document["accessors"][primitive["indices"]]["count"])
            else:
                count = int(document["accessors"][primitive["attributes"]["POSITION"]]["count"])
            require(count % 3 == 0, "triangle index count is not divisible by three")
            triangles += count // 3
    return triangles


def node_parent_map(document: dict[str, Any]) -> dict[int, int]:
    parents: dict[int, int] = {}
    for parent_index, node in enumerate(document.get("nodes", [])):
        for child in node.get("children", []):
            require(child not in parents, f"node {child} has multiple parents")
            parents[int(child)] = parent_index
    return parents


def skin_hierarchy(document: dict[str, Any]) -> tuple[tuple[str, str], ...]:
    require(len(document.get("skins", [])) == 1, "character GLB must have exactly one skin")
    joints = [int(index) for index in document["skins"][0]["joints"]]
    joint_set = set(joints)
    parents = node_parent_map(document)
    names = {index: str(document["nodes"][index].get("name", "")) for index in joints}
    hierarchy = []
    for index in joints:
        parent = parents.get(index)
        parent_name = names[parent] if parent in joint_set else ""
        hierarchy.append((names[index], parent_name))
    return tuple(hierarchy)


def validate_skin_weights(document: dict[str, Any], binary: bytes, path: Path) -> None:
    joint_count = len(document["skins"][0]["joints"])
    weighted_vertices = 0
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            attributes = primitive.get("attributes", {})
            require("JOINTS_0" in attributes and "WEIGHTS_0" in attributes, f"{path} has an unskinned primitive")
            joints = accessor_values(document, binary, int(attributes["JOINTS_0"]))
            weights = accessor_values(document, binary, int(attributes["WEIGHTS_0"]))
            require(len(joints) == len(weights), f"{path} joint/weight accessor counts differ")
            for vertex_joints, vertex_weights in zip(joints, weights, strict=True):
                require(all(0 <= int(joint) < joint_count for joint in vertex_joints), f"{path} has an invalid joint index")
                require(all(weight >= -1.0e-6 for weight in vertex_weights), f"{path} has a negative skin weight")
                require(abs(sum(vertex_weights) - 1.0) <= 1.0e-3, f"{path} has non-normalized skin weights")
                require(sum(weight > 1.0e-6 for weight in vertex_weights) <= 4, f"{path} exceeds four skin influences")
                weighted_vertices += 1
    require(weighted_vertices > 0, f"{path} contains no weighted vertices")


def validate_animation_sampling(
    document: dict[str, Any], binary: bytes, path: Path
) -> None:
    root_indices = {
        index for index, node in enumerate(document.get("nodes", [])) if node.get("name") == "Root"
    }
    for animation in document.get("animations", []):
        for sampler in animation.get("samplers", []):
            times = [value[0] for value in accessor_values(document, binary, int(sampler["input"]))]
            require(times == sorted(times), f"{path} animation time samples are unsorted")
            for left, right in zip(times, times[1:]):
                frames = (right - left) * recipes.ANIMATION_FPS
                require(abs(frames - round(frames)) <= 2.0e-4, f"{path} is not sampled at 30 FPS")
        for channel in animation.get("channels", []):
            target = channel.get("target", {})
            if int(target.get("node", -1)) not in root_indices or target.get("path") != "translation":
                continue
            sampler = animation["samplers"][int(channel["sampler"])]
            translations = accessor_values(document, binary, int(sampler["output"]))
            require(translations, f"{path} has an empty root translation channel")
            first = translations[0]
            require(
                all(
                    all(abs(component - initial) <= 1.0e-6 for component, initial in zip(value, first, strict=True))
                    for value in translations
                ),
                f"{path} animation {animation.get('name')} moves its root",
            )


def validate_source_manifest(root: Path, manifest_hashes: dict[str, str]) -> None:
    path = root / recipes.SOURCE_MANIFEST_PATH
    with path.open("rb") as handle:
        manifest = tomllib.load(handle)
    expected_hashes = {
        key.removesuffix("_sha256"): value for key, value in manifest_hashes.items()
    }
    sources = {entry["id"]: entry for entry in manifest.get("source", [])}
    require(set(sources) == set(expected_hashes), "3D source coverage differs from generated manifest")
    for source_id, expected in expected_hashes.items():
        source = sources[source_id]
        source_path = root / source["path"]
        require(source.get("ready") is True, f"source {source_id} is not ready")
        require(source.get("sha256") == expected, f"source {source_id} hash differs between manifests")
        require(sha256_file(source_path) == expected, f"source {source_id} bytes do not match manifest")
        inputs = source.get("inputs", [])
        input_hashes = source.get("input_sha256", [])
        require(
            bool(inputs) and len(inputs) == len(input_hashes),
            f"source {source_id} must hash every generation input",
        )
        for input_path, input_hash in zip(inputs, input_hashes, strict=True):
            path = root / input_path
            require(path.is_file(), f"source input is missing: {path}")
            require(
                sha256_file(path) == input_hash,
                f"source input hash mismatch for {path}",
            )


def validate_materials(root: Path, entries: Sequence[dict[str, Any]]) -> None:
    expected = {material.id: material for material in recipes.MATERIALS}
    require({entry["id"] for entry in entries} == set(expected), "material atlas coverage is incomplete")
    for entry in entries:
        require(entry.get("ready") is True, f"material {entry['id']} is not ready")
        require((entry.get("width"), entry.get("height")) == (256, 256), f"material {entry['id']} has wrong dimensions")
        for field, hash_field in (
            ("base_color", "base_color_sha256"),
            ("normal", "normal_sha256"),
            ("orm", "orm_sha256"),
        ):
            path = root / entry[field]
            require(path.is_file(), f"missing material output {path}")
            require(sha256_file(path) == entry[hash_field], f"material hash mismatch for {path}")
            image = bpy.data.images.load(str(path), check_existing=False)
            try:
                require(tuple(image.size) == (256, 256), f"{path} is not 256x256")
            finally:
                bpy.data.images.remove(image)


def validate_models(root: Path, entries: Sequence[dict[str, Any]]) -> None:
    expected = recipes.MODEL_BY_ID
    require(len(entries) == 35, f"expected 35 models, found {len(entries)}")
    require({entry["id"] for entry in entries} == set(expected), "model coverage differs from recipes")
    reference_hierarchy: tuple[tuple[str, str], ...] | None = None

    for entry in entries:
        recipe = expected[entry["id"]]
        path = root / entry["path"]
        require(entry.get("ready") is True, f"model {recipe.id} is not ready")
        require(entry["family"] == recipe.family and entry["variant"] == recipe.variant, f"model identity drift for {recipe.id}")
        require(tuple(entry["footprint"]) == recipe.footprint, f"footprint drift for {recipe.id}")
        require(tuple(entry["materials"]) == recipe.materials, f"material contract drift for {recipe.id}")
        require(entry["triangle_budget"] == recipe.triangle_budget, f"triangle budget drift for {recipe.id}")
        require(entry.get("pivot") == [0.0, 0.0, 0.0], f"{recipe.id} pivot must be the ground origin")
        require(entry["bounds_min"][1] >= -0.03, f"{recipe.id} extends below its pivot")
        require(path.is_file(), f"missing model {path}")
        require(sha256_file(path) == entry["sha256"], f"model hash mismatch for {path}")
        document, binary = load_glb(path)
        if recipe.family in {"building", "npc", "wheelbarrow"}:
            require(
                any(
                    node.get("extras", {}).get("boapspace_forward") == "-Z"
                    for node in document.get("nodes", [])
                ),
                f"{recipe.id} has no validated -Z forward marker",
            )
        if recipe.family in {"building", "resource", "crop", "tree"}:
            mesh_nodes = [node for node in document.get("nodes", []) if "mesh" in node]
            require(
                len(mesh_nodes) == 1,
                f"{recipe.id} must export exactly one batchable mesh node",
            )
            node = mesh_nodes[0]
            require(
                "matrix" not in node
                and tuple(node.get("translation", (0.0, 0.0, 0.0))) == (0.0, 0.0, 0.0)
                and tuple(node.get("rotation", (0.0, 0.0, 0.0, 1.0)))
                == (0.0, 0.0, 0.0, 1.0)
                and tuple(node.get("scale", (1.0, 1.0, 1.0))) == (1.0, 1.0, 1.0),
                f"{recipe.id} mesh node does not use the declared ground-origin pivot",
            )
        triangles = primitive_triangle_count(document)
        require(triangles == entry["triangle_count"], f"triangle count drift for {recipe.id}")
        require(0 < triangles <= recipe.triangle_budget, f"triangle budget exceeded for {recipe.id}")
        material_names = tuple(material.get("name", "") for material in document.get("materials", []))
        require(set(material_names) == set(recipe.materials), f"GLB material slots differ for {recipe.id}: {material_names}")
        expected_image_uris = {
            f"../materials/{material}_{role}.png"
            for material in recipe.materials
            for role in ("base_color", "normal", "orm")
        }
        images = document.get("images", [])
        require(
            {image.get("uri") for image in images} == expected_image_uris,
            f"{recipe.id} does not reference exactly its shared material atlases",
        )
        require(
            all("bufferView" not in image for image in images),
            f"{recipe.id} embeds duplicate texture images",
        )
        animation_names = tuple(animation.get("name", "") for animation in document.get("animations", []))
        require(set(animation_names) == set(recipe.animations), f"animation set differs for {recipe.id}: {animation_names}")
        if recipe.wrapper is not None:
            require(entry.get("wrapper") == recipe.wrapper.as_posix(), f"wrapper path drift for {recipe.id}")
            require((root / recipe.wrapper).is_file(), f"missing wrapper {recipe.wrapper}")
        if recipe.family == "npc":
            require(entry.get("skeleton") == recipes.SKELETON_ID, f"skeleton id drift for {recipe.id}")
            hierarchy = skin_hierarchy(document)
            require(
                hierarchy == tuple((name, parent or "") for name, parent in recipes.SKELETON_BONES),
                f"canonical hierarchy differs for {recipe.id}: {hierarchy}",
            )
            if reference_hierarchy is None:
                reference_hierarchy = hierarchy
            else:
                require(hierarchy == reference_hierarchy, f"shared bone hierarchy drift for {recipe.id}")
            validate_skin_weights(document, binary, path)
            validate_animation_sampling(document, binary, path)
        elif recipe.family == "wheelbarrow":
            validate_animation_sampling(document, binary, path)


def validate_manifest(root: Path, manifest_path: Path) -> None:
    with manifest_path.open("rb") as handle:
        manifest = tomllib.load(handle)
    require(manifest.get("schema_version") == recipes.SCHEMA_VERSION, "unsupported 3D manifest schema")
    require(manifest.get("blender_version") == recipes.BLENDER_VERSION, "Blender version drift")
    require(manifest.get("godot_version") == recipes.GODOT_VERSION, "Godot version drift")
    require(manifest.get("tile_units") == recipes.TILE_UNITS, "3D tile scale drift")
    require(manifest.get("subtile_units_per_tile") == recipes.SUBTILE_UNITS_PER_TILE, "subtile scale drift")
    require(manifest.get("up_axis") == "+Y" and manifest.get("forward_axis") == "-Z", "axis contract drift")
    require(manifest.get("animation_fps") == recipes.ANIMATION_FPS, "animation FPS drift")
    require(manifest.get("ready") is True, "3D manifest is not ready")
    validate_source_manifest(root, manifest["sources"])
    skeleton = manifest["skeleton"]
    require(tuple(skeleton["bones"]) == tuple(name for name, _ in recipes.SKELETON_BONES), "manifest bones drift")
    require(tuple(skeleton["parents"]) == tuple(parent or "" for _, parent in recipes.SKELETON_BONES), "manifest hierarchy drift")
    require(tuple(skeleton["animations"]) == recipes.NPC_ANIMATIONS, "manifest animation contract drift")
    require(skeleton.get("stationary_root") is True, "manifest must require stationary roots")
    validate_materials(root, manifest.get("material", []))
    validate_models(root, manifest.get("model", []))


def main() -> None:
    arguments = parse_arguments()
    actual_version = ".".join(str(part) for part in bpy.app.version)
    if actual_version != recipes.BLENDER_VERSION and not arguments.skip_version_check:
        raise RuntimeError(
            f"validation requires Blender {recipes.BLENDER_VERSION}, found {actual_version}"
        )
    root = arguments.root.resolve()
    manifest_path = arguments.manifest
    if not manifest_path.is_absolute():
        manifest_path = root / manifest_path
    validate_manifest(root, manifest_path)
    print("validated 35 GLBs, five material atlas sets, canonical rigs, and animations")


if __name__ == "__main__":
    main()
