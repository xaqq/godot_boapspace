"""Generate every shipping asset for the experimental 3D renderer.

Run from the repository root with the pinned Blender version::

    blender --background --factory-startup \
      --python tools/assets_3d/generate.py

Approved raster sources are immutable inputs.  Their hashes must be recorded
in ``art_sources/world_3d/source_manifest.toml`` before this script will write
shipping output.  Geometry is intentionally recipe-driven: regeneration is
deterministic and does not depend on interactive Blender state or ``.blend``
files.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

import bpy
from mathutils import Vector

SCRIPT_DIR = Path(__file__).resolve().parent
REPOSITORY_ROOT = SCRIPT_DIR.parents[1]
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import recipes  # noqa: E402  (must follow Blender/script path setup)


TEXTURE_SIZE = 256
@dataclass(frozen=True)
class ApprovedSource:
    id: str
    path: Path
    sha256: str


@dataclass(frozen=True)
class MaterialRecord:
    recipe: recipes.MaterialRecipe
    base_color_sha256: str
    normal_sha256: str
    orm_sha256: str


@dataclass(frozen=True)
class ModelRecord:
    recipe: recipes.ModelRecipe
    sha256: str
    provenance_sha256: str
    triangle_count: int
    bounds_min: tuple[float, float, float]
    bounds_max: tuple[float, float, float]


def parse_arguments() -> argparse.Namespace:
    blender_args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-root",
        type=Path,
        default=REPOSITORY_ROOT,
        help="repository containing approved sources (defaults to this checkout)",
    )
    parser.add_argument(
        "--output-root",
        type=Path,
        default=REPOSITORY_ROOT,
        help="repository-shaped output root (defaults to this checkout)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="regenerate into a temporary directory and byte-compare shipping output",
    )
    parser.add_argument(
        "--skip-version-check",
        action="store_true",
        help="development escape hatch; never use for committed shipping assets",
    )
    return parser.parse_args(blender_args)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_blender_version(skip: bool) -> None:
    actual = ".".join(str(part) for part in bpy.app.version)
    if actual != recipes.BLENDER_VERSION and not skip:
        raise RuntimeError(
            f"asset generation requires Blender {recipes.BLENDER_VERSION}, found {actual}"
        )


def load_approved_sources(source_root: Path) -> dict[str, ApprovedSource]:
    manifest_path = source_root / recipes.SOURCE_MANIFEST_PATH
    if not manifest_path.is_file():
        raise RuntimeError(f"missing source manifest: {manifest_path}")
    with manifest_path.open("rb") as handle:
        manifest = tomllib.load(handle)
    if manifest.get("schema_version") != recipes.SCHEMA_VERSION:
        raise RuntimeError(f"unsupported source manifest schema in {manifest_path}")

    approved: dict[str, ApprovedSource] = {}
    for entry in manifest.get("source", []):
        source_id = str(entry.get("id", ""))
        path = source_root / str(entry.get("path", ""))
        expected = str(entry.get("sha256", ""))
        if source_id in approved:
            raise RuntimeError(f"duplicate source id {source_id!r}")
        if not entry.get("ready", False):
            raise RuntimeError(
                f"source {source_id!r} is not approved; set ready only after visual review"
            )
        input_paths = [source_root / str(value) for value in entry.get("inputs", [])]
        input_hashes = [str(value) for value in entry.get("input_sha256", [])]
        if not input_paths or len(input_paths) != len(input_hashes):
            raise RuntimeError(
                f"source {source_id!r} must record one SHA-256 for every generation input"
            )
        for input_path, input_hash in zip(input_paths, input_hashes, strict=True):
            if not input_path.is_file():
                raise RuntimeError(f"generation input is missing: {input_path}")
            actual_input_hash = sha256_file(input_path)
            if actual_input_hash != input_hash:
                raise RuntimeError(
                    f"generation input hash mismatch for {input_path}: "
                    f"expected {input_hash}, got {actual_input_hash}"
                )
        if len(expected) != 64:
            raise RuntimeError(f"source {source_id!r} has no approved SHA-256")
        if not path.is_file():
            raise RuntimeError(f"approved source is missing: {path}")
        actual = sha256_file(path)
        if actual != expected:
            raise RuntimeError(
                f"approved source hash mismatch for {path}: expected {expected}, got {actual}"
            )
        approved[source_id] = ApprovedSource(source_id, path, actual)

    expected_paths = {
        "model_turnaround": source_root / recipes.TURNAROUND_SOURCE_PATH,
        "material_source": source_root / recipes.MATERIAL_SOURCE_PATH,
    }
    if set(approved) != set(expected_paths):
        raise RuntimeError(
            f"source manifest must contain exactly {sorted(expected_paths)}, got {sorted(approved)}"
        )
    for source_id, expected_path in expected_paths.items():
        if approved[source_id].path.resolve() != expected_path.resolve():
            raise RuntimeError(f"source {source_id!r} must use {expected_path}")
    return approved


def clean_scene() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = recipes.ANIMATION_FPS
    scene.render.fps_base = 1.0
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = 1.0


def save_rgba_image(
    name: str,
    path: Path,
    width: int,
    height: int,
    pixels: Sequence[float],
    *,
    color_space: str,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    image = bpy.data.images.new(name, width=width, height=height, alpha=True, float_buffer=False)
    image.colorspace_settings.name = color_space
    image.pixels.foreach_set(pixels)
    image.file_format = "PNG"
    image.filepath_raw = str(path)
    image.save()
    bpy.data.images.remove(image)


def sample_material_band(
    source_pixels: Sequence[float],
    source_width: int,
    source_height: int,
    band_index: int,
    tint: tuple[float, float, float],
) -> list[float]:
    band_count = len(recipes.MATERIALS)
    band_start = source_width * band_index // band_count
    band_end = source_width * (band_index + 1) // band_count
    band_width = max(1, band_end - band_start)
    output = [0.0] * (TEXTURE_SIZE * TEXTURE_SIZE * 4)
    for y in range(TEXTURE_SIZE):
        source_y = min(source_height - 1, y * source_height // TEXTURE_SIZE)
        for x in range(TEXTURE_SIZE):
            source_x = band_start + min(band_width - 1, x * band_width // TEXTURE_SIZE)
            source_offset = (source_y * source_width + source_x) * 4
            target_offset = (y * TEXTURE_SIZE + x) * 4
            for channel in range(3):
                value = float(source_pixels[source_offset + channel]) * tint[channel]
                output[target_offset + channel] = min(1.0, max(0.0, value))
            output[target_offset + 3] = 1.0
    return output


def normal_pixels(base: Sequence[float]) -> list[float]:
    luminance = [0.0] * (TEXTURE_SIZE * TEXTURE_SIZE)
    for index in range(TEXTURE_SIZE * TEXTURE_SIZE):
        offset = index * 4
        luminance[index] = (
            base[offset] * 0.2126 + base[offset + 1] * 0.7152 + base[offset + 2] * 0.0722
        )

    def height_at(x: int, y: int) -> float:
        return luminance[(y % TEXTURE_SIZE) * TEXTURE_SIZE + (x % TEXTURE_SIZE)]

    output = [0.0] * (TEXTURE_SIZE * TEXTURE_SIZE * 4)
    strength = 2.0
    for y in range(TEXTURE_SIZE):
        for x in range(TEXTURE_SIZE):
            dx = (height_at(x + 1, y) - height_at(x - 1, y)) * strength
            dy = (height_at(x, y + 1) - height_at(x, y - 1)) * strength
            normal = Vector((-dx, -dy, 1.0)).normalized()
            offset = (y * TEXTURE_SIZE + x) * 4
            output[offset] = normal.x * 0.5 + 0.5
            output[offset + 1] = normal.y * 0.5 + 0.5
            output[offset + 2] = normal.z * 0.5 + 0.5
            output[offset + 3] = 1.0
    return output


def orm_pixels(
    base: Sequence[float], roughness: float, metallic: float
) -> list[float]:
    output = [0.0] * (TEXTURE_SIZE * TEXTURE_SIZE * 4)
    for index in range(TEXTURE_SIZE * TEXTURE_SIZE):
        offset = index * 4
        luminance = (
            base[offset] * 0.2126 + base[offset + 1] * 0.7152 + base[offset + 2] * 0.0722
        )
        variation = (luminance - 0.5) * 0.18
        output[offset] = min(1.0, max(0.0, 0.92 + variation * 0.25))
        output[offset + 1] = min(1.0, max(0.0, roughness - variation))
        output[offset + 2] = metallic
        output[offset + 3] = 1.0
    return output


def generate_material_textures(
    source: ApprovedSource, output_root: Path
) -> tuple[MaterialRecord, ...]:
    clean_scene()
    source_image = bpy.data.images.load(str(source.path), check_existing=False)
    source_width, source_height = (int(value) for value in source_image.size)
    if source_width < len(recipes.MATERIALS) or source_height < 1:
        raise RuntimeError(
            f"material source {source.path} is too small for five horizontal swatches"
        )
    source_pixels = list(source_image.pixels[:])
    bpy.data.images.remove(source_image)

    records: list[MaterialRecord] = []
    for band_index, material in enumerate(recipes.MATERIALS):
        base = sample_material_band(
            source_pixels, source_width, source_height, band_index, material.tint
        )
        base_path = output_root / material.base_color_path
        normal_path = output_root / material.normal_path
        orm_path = output_root / material.orm_path
        save_rgba_image(
            f"{material.id}_base_color",
            base_path,
            TEXTURE_SIZE,
            TEXTURE_SIZE,
            base,
            color_space="sRGB",
        )
        save_rgba_image(
            f"{material.id}_normal",
            normal_path,
            TEXTURE_SIZE,
            TEXTURE_SIZE,
            normal_pixels(base),
            color_space="Non-Color",
        )
        save_rgba_image(
            f"{material.id}_orm",
            orm_path,
            TEXTURE_SIZE,
            TEXTURE_SIZE,
            orm_pixels(base, material.roughness, material.metallic),
            color_space="Non-Color",
        )
        records.append(
            MaterialRecord(
                recipe=material,
                base_color_sha256=sha256_file(base_path),
                normal_sha256=sha256_file(normal_path),
                orm_sha256=sha256_file(orm_path),
            )
        )
    return tuple(records)


def load_materials(output_root: Path) -> dict[str, bpy.types.Material]:
    loaded: dict[str, bpy.types.Material] = {}
    for recipe in recipes.MATERIALS:
        material = bpy.data.materials.new(recipe.id)
        material.use_nodes = True
        material.diffuse_color = (*recipe.tint, 1.0)
        nodes = material.node_tree.nodes
        links = material.node_tree.links
        nodes.clear()
        output = nodes.new("ShaderNodeOutputMaterial")
        shader = nodes.new("ShaderNodeBsdfPrincipled")
        base_texture = nodes.new("ShaderNodeTexImage")
        normal_texture = nodes.new("ShaderNodeTexImage")
        orm_texture = nodes.new("ShaderNodeTexImage")
        normal_map = nodes.new("ShaderNodeNormalMap")
        separate = nodes.new("ShaderNodeSeparateColor")

        base_texture.image = bpy.data.images.load(
            str(output_root / recipe.base_color_path), check_existing=True
        )
        normal_texture.image = bpy.data.images.load(
            str(output_root / recipe.normal_path), check_existing=True
        )
        orm_texture.image = bpy.data.images.load(
            str(output_root / recipe.orm_path), check_existing=True
        )
        base_texture.image.colorspace_settings.name = "sRGB"
        normal_texture.image.colorspace_settings.name = "Non-Color"
        orm_texture.image.colorspace_settings.name = "Non-Color"
        normal_map.inputs["Strength"].default_value = 0.35

        links.new(base_texture.outputs["Color"], shader.inputs["Base Color"])
        links.new(normal_texture.outputs["Color"], normal_map.inputs["Color"])
        links.new(normal_map.outputs["Normal"], shader.inputs["Normal"])
        links.new(orm_texture.outputs["Color"], separate.inputs["Color"])
        links.new(separate.outputs["Green"], shader.inputs["Roughness"])
        links.new(separate.outputs["Blue"], shader.inputs["Metallic"])
        links.new(shader.outputs["BSDF"], output.inputs["Surface"])
        loaded[recipe.id] = material
    return loaded


def assign_material(obj: bpy.types.Object, material: bpy.types.Material) -> None:
    if obj.type != "MESH":
        return
    obj.data.materials.clear()
    obj.data.materials.append(material)


def apply_object_transform(obj: bpy.types.Object) -> None:
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.select_set(False)


def add_box(
    name: str,
    dimensions: tuple[float, float, float],
    location: tuple[float, float, float],
    material: bpy.types.Material,
    rotation: tuple[float, float, float] = (0.0, 0.0, 0.0),
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_cube_add(location=location, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.scale = tuple(value * 0.5 for value in dimensions)
    apply_object_transform(obj)
    assign_material(obj, material)
    return obj


def add_cylinder(
    name: str,
    radius: float,
    depth: float,
    location: tuple[float, float, float],
    material: bpy.types.Material,
    *,
    vertices: int = 10,
    rotation: tuple[float, float, float] = (0.0, 0.0, 0.0),
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=vertices, radius=radius, depth=depth, location=location, rotation=rotation
    )
    obj = bpy.context.object
    obj.name = name
    apply_object_transform(obj)
    assign_material(obj, material)
    return obj


def add_cone(
    name: str,
    radius1: float,
    radius2: float,
    depth: float,
    location: tuple[float, float, float],
    material: bpy.types.Material,
    *,
    vertices: int = 8,
    rotation: tuple[float, float, float] = (0.0, 0.0, 0.0),
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_cone_add(
        vertices=vertices,
        radius1=radius1,
        radius2=radius2,
        depth=depth,
        location=location,
        rotation=rotation,
    )
    obj = bpy.context.object
    obj.name = name
    apply_object_transform(obj)
    assign_material(obj, material)
    return obj


def add_rock(
    name: str,
    radius: float,
    location: tuple[float, float, float],
    material: bpy.types.Material,
    scale: tuple[float, float, float] = (1.0, 0.8, 0.7),
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=radius, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    apply_object_transform(obj)
    assign_material(obj, material)
    return obj


def add_uv_sphere(
    name: str,
    radius: float,
    location: tuple[float, float, float],
    material: bpy.types.Material,
    scale: tuple[float, float, float] = (1.0, 1.0, 1.0),
) -> bpy.types.Object:
    bpy.ops.mesh.primitive_uv_sphere_add(
        segments=12, ring_count=6, radius=radius, location=location
    )
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    apply_object_transform(obj)
    assign_material(obj, material)
    return obj


def add_segment(
    name: str,
    start: tuple[float, float, float],
    end: tuple[float, float, float],
    radius: float,
    material: bpy.types.Material,
    *,
    vertices: int = 8,
) -> bpy.types.Object:
    start_vector = Vector(start)
    end_vector = Vector(end)
    direction = end_vector - start_vector
    midpoint = (start_vector + end_vector) * 0.5
    obj = add_cylinder(
        name,
        radius,
        direction.length,
        tuple(midpoint),
        material,
        vertices=vertices,
    )
    obj.rotation_mode = "QUATERNION"
    obj.rotation_quaternion = direction.to_track_quat("Z", "Y")
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    obj.select_set(False)
    return obj


def join_meshes(name: str) -> bpy.types.Object:
    meshes = sorted(
        (obj for obj in bpy.context.scene.objects if obj.type == "MESH"), key=lambda obj: obj.name
    )
    if not meshes:
        raise RuntimeError(f"cannot create {name}: scene has no mesh objects")
    bpy.ops.object.select_all(action="DESELECT")
    for obj in meshes:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.join()
    joined = bpy.context.object
    joined.name = name
    # Joining keeps the active object's origin and moves the other vertices
    # into that local space. Bake the resulting object transform so static GLB
    # mesh nodes use the manifest's ground-origin pivot instead of carrying an
    # arbitrary translation from whichever source primitive sorted first.
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return joined


def mark_and_align_forward(obj: bpy.types.Object) -> None:
    """Bake authored Blender -Y fronts to the shipping Godot -Z contract.

    Blender +Y becomes Godot -Z under glTF's Y-up conversion. Most authored
    building details use Blender -Y as their visual front, so rotate those
    complete meshes before export and record a machine-checkable marker.
    """

    obj.rotation_euler.z = math.pi
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
    obj.select_set(False)
    obj["boapspace_forward"] = "-Z"


def add_plot_frame(
    width: float,
    depth: float,
    materials: dict[str, bpy.types.Material],
    *,
    soil_material: str,
) -> None:
    add_box(
        "GroundBed",
        (width * 0.88, depth * 0.88, 0.08),
        (0.0, 0.0, 0.04),
        materials[soil_material],
    )
    border = 0.10
    inset_x = width * 0.44
    inset_y = depth * 0.44
    add_box("BorderNorth", (width * 0.92, border, 0.12), (0.0, -inset_y, 0.08), materials["timber"])
    add_box("BorderSouth", (width * 0.92, border, 0.12), (0.0, inset_y, 0.08), materials["timber"])
    add_box("BorderWest", (border, depth * 0.92, 0.12), (-inset_x, 0.0, 0.08), materials["timber"])
    add_box("BorderEast", (border, depth * 0.92, 0.12), (inset_x, 0.0, 0.08), materials["timber"])


def build_building(
    recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]
) -> None:
    width = recipe.footprint[0] * recipes.TILE_UNITS
    depth = recipe.footprint[1] * recipes.TILE_UNITS
    if recipe.variant in {"Field", "TreePlot"}:
        add_plot_frame(width, depth, materials, soil_material="organic")
        mark_and_align_forward(join_meshes("BuildingMesh"))
        return

    primary, secondary = recipe.materials[:2]
    primary_material = materials[primary]
    secondary_material = materials[secondary]
    maximum = max(recipe.footprint)
    body_height = 0.72 + maximum * 0.22
    add_box("Foundation", (width * 0.90, depth * 0.90, 0.16), (0.0, 0.0, 0.08), secondary_material)
    add_box(
        "MainBody",
        (width * 0.72, depth * 0.70, body_height),
        (0.0, 0.0, 0.16 + body_height * 0.5),
        primary_material,
    )
    roof_height = 0.32 + maximum * 0.08
    add_cone(
        "Roof",
        max(width, depth) * 0.51,
        max(width, depth) * 0.10,
        roof_height,
        (0.0, 0.0, 0.16 + body_height + roof_height * 0.5),
        secondary_material,
        vertices=4,
        rotation=(0.0, 0.0, math.pi * 0.25),
    )
    # A recessed entrance gives every building an unambiguous front (-Y).
    add_box(
        "Entrance",
        (min(0.65, width * 0.25), 0.10, body_height * 0.58),
        (0.0, -depth * 0.355, 0.16 + body_height * 0.29),
        secondary_material,
    )

    top = 0.16 + body_height
    if recipe.variant == "Depot":
        add_box("Canopy", (width * 0.62, 0.55, 0.10), (0.0, -depth * 0.42, top * 0.62), secondary_material)
        add_box("CargoBand", (width * 0.54, 0.12, 0.18), (0.0, -depth * 0.38, top * 0.82), primary_material)
    elif recipe.variant == "Warehouse":
        for index, x in enumerate((-width * 0.24, 0.0, width * 0.24)):
            add_cylinder(f"RoofVent{index}", 0.13, 0.32, (x, 0.0, top + roof_height + 0.12), primary_material, vertices=8)
        add_box("LoadingDoor", (width * 0.34, 0.12, body_height * 0.72), (0.0, -depth * 0.36, 0.16 + body_height * 0.36), secondary_material)
    elif recipe.variant == "TownHall":
        add_box("Tower", (width * 0.24, depth * 0.24, 0.72), (0.0, 0.0, top + 0.25), primary_material)
        add_cone("Beacon", 0.20, 0.03, 0.36, (0.0, 0.0, top + 0.78), secondary_material, vertices=8)
    elif recipe.variant == "Sawmill":
        for index, y in enumerate((-0.42, 0.0, 0.42)):
            add_cylinder(f"Log{index}", 0.13, width * 0.62, (0.0, y, 0.32), secondary_material, rotation=(0.0, math.pi * 0.5, 0.0))
        add_cylinder("SawWheel", 0.38, 0.08, (width * 0.31, -0.15, 0.62), primary_material, vertices=16, rotation=(math.pi * 0.5, 0.0, 0.0))
    elif recipe.variant == "Stoneworks":
        for index, (x, y) in enumerate(((-0.45, 0.34), (0.0, 0.42), (0.45, 0.34))):
            add_box(f"StoneBlock{index}", (0.36, 0.34, 0.32), (x, y, 0.24), secondary_material)
        add_cylinder("DustStack", 0.18, 0.72, (width * 0.24, depth * 0.18, top + 0.15), primary_material, vertices=8)
    elif recipe.variant == "Kitchen":
        add_cylinder("Chimney", 0.18, 0.82, (width * 0.22, depth * 0.16, top + 0.22), secondary_material, vertices=8)
        add_cylinder("CookPot", 0.30, 0.28, (-width * 0.25, -depth * 0.32, 0.30), primary_material, vertices=10)
    elif recipe.variant == "Farm":
        add_cylinder("Silo", width * 0.14, 1.25, (width * 0.27, depth * 0.20, 0.78), primary_material, vertices=12)
        add_cone("SiloRoof", width * 0.17, 0.02, 0.30, (width * 0.27, depth * 0.20, 1.55), secondary_material, vertices=12)
        add_box("FeedAwning", (width * 0.34, 0.48, 0.10), (-width * 0.24, -depth * 0.40, 0.65), secondary_material)
    elif recipe.variant == "ForesterLodge":
        for index, x in enumerate((-0.46, 0.0, 0.46)):
            add_cylinder(f"TimberStack{index}", 0.13, 0.90, (x, depth * 0.30, 0.27), secondary_material, rotation=(math.pi * 0.5, 0.0, 0.0))
        add_cone("LookoutCap", 0.34, 0.04, 0.42, (-width * 0.24, 0.0, top + 0.34), secondary_material, vertices=8)
    else:
        # Houses share a family language but the number of dormers communicates capacity.
        dormers = {"SmallHouse": 1, "MediumHouse": 2, "LargeHouse": 3}[recipe.variant]
        for index in range(dormers):
            x = (index - (dormers - 1) * 0.5) * min(0.70, width * 0.20)
            add_box(f"Dormer{index}", (0.30, 0.34, 0.26), (x, -depth * 0.16, top + roof_height * 0.42), primary_material)

    mark_and_align_forward(join_meshes("BuildingMesh"))


def build_resource(
    recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]
) -> None:
    variant = recipe.variant
    if variant == "Wood":
        for index, (y, z) in enumerate(((-0.24, 0.20), (0.04, 0.20), (-0.10, 0.46))):
            add_cylinder(f"Log{index}", 0.16, 1.15, (0.0, y, z), materials["timber"], rotation=(0.0, math.pi * 0.5, 0.0))
    elif variant == "Stone":
        for index, (x, y, radius) in enumerate(((-0.30, 0.0, 0.34), (0.24, -0.10, 0.40), (0.04, 0.30, 0.28))):
            add_rock(f"Stone{index}", radius, (x, y, radius * 0.75), materials["masonry_ore"])
    elif variant == "Food":
        add_box("FoodCrate", (1.05, 0.82, 0.34), (0.0, 0.0, 0.17), materials["structure"])
        for index, (x, y) in enumerate(((-0.28, -0.12), (0.02, 0.04), (0.29, -0.08), (-0.10, 0.24))):
            add_uv_sphere(f"Food{index}", 0.17, (x, y, 0.43), materials["organic"], (1.0, 0.85, 0.80))
    elif variant == "Gold":
        for index, (x, y, radius) in enumerate(((-0.28, -0.10, 0.32), (0.22, -0.06, 0.38), (0.02, 0.30, 0.27))):
            material = materials["structure"] if index == 1 else materials["masonry_ore"]
            add_rock(f"GoldOre{index}", radius, (x, y, radius * 0.75), material, (1.0, 0.75, 0.65))
    elif variant in {"Crops", "WildBerries"}:
        for index, (x, y) in enumerate(((-0.32, -0.22), (0.0, -0.28), (0.31, -0.10), (-0.20, 0.18), (0.18, 0.24))):
            height = 0.55 if variant == "Crops" else 0.42
            add_cylinder(f"Stem{index}", 0.045, height, (x, y, height * 0.5), materials["organic"], vertices=6)
            add_uv_sphere(f"Head{index}", 0.13, (x, y, height + 0.04), materials["organic"], (1.0, 0.85, 0.75))
    elif variant == "Planks":
        for layer in range(3):
            for column in range(2):
                add_box(
                    f"Plank{layer}_{column}",
                    (1.18, 0.20, 0.12),
                    (0.0, (column - 0.5) * 0.25, 0.08 + layer * 0.13),
                    materials["timber"],
                    rotation=(0.0, 0.0, (layer % 2) * 0.04),
                )
    elif variant == "StoneBlocks":
        for layer in range(2):
            for column in range(3):
                add_box(
                    f"Block{layer}_{column}",
                    (0.38, 0.55, 0.28),
                    ((column - 1) * 0.40 + (0.20 if layer else 0.0), 0.0, 0.14 + layer * 0.29),
                    materials["masonry_ore"],
                )
    else:
        raise AssertionError(f"unhandled resource variant {variant}")
    join_meshes("ResourceMesh")


def add_crop_stalk(
    name: str,
    x: float,
    y: float,
    height: float,
    materials: dict[str, bpy.types.Material],
    *,
    mature: bool,
) -> None:
    add_cylinder(name, 0.035, height, (x, y, height * 0.5 + 0.08), materials["organic"], vertices=6)
    add_uv_sphere(
        f"{name}Leaves",
        0.10 if not mature else 0.15,
        (x, y, height + 0.08),
        materials["organic"],
        (1.0, 0.70, 0.55),
    )


def build_crop(recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]) -> None:
    add_plot_frame(recipes.TILE_UNITS, recipes.TILE_UNITS, materials, soil_material="organic")
    counts = {"Seedable": 0, "GrowingStep1": 5, "GrowingStep2": 9, "Grown": 13}
    count = counts[recipe.variant]
    if count:
        positions = [
            ((index % 4) - 1.5, (index // 4) - 1.5)
            for index in range(count)
        ]
        height = {"GrowingStep1": 0.26, "GrowingStep2": 0.48, "Grown": 0.72}[recipe.variant]
        for index, (column, row) in enumerate(positions):
            add_crop_stalk(
                f"Crop{index}",
                column * 0.34,
                row * 0.34,
                height,
                materials,
                mature=recipe.variant == "Grown",
            )
    join_meshes("CropMesh")


def add_tree(
    name: str,
    height: float,
    crown_radius: float,
    materials: dict[str, bpy.types.Material],
) -> None:
    trunk_height = height * 0.58
    add_cylinder(f"{name}Trunk", crown_radius * 0.20, trunk_height, (0.0, 0.0, trunk_height * 0.5 + 0.08), materials["timber"], vertices=8)
    add_cone(
        f"{name}CrownLow",
        crown_radius,
        crown_radius * 0.28,
        height * 0.48,
        (0.0, 0.0, trunk_height + height * 0.16),
        materials["organic"],
        vertices=9,
    )
    add_cone(
        f"{name}CrownHigh",
        crown_radius * 0.72,
        0.03,
        height * 0.38,
        (0.0, 0.0, trunk_height + height * 0.43),
        materials["organic"],
        vertices=9,
    )


def build_tree(recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]) -> None:
    sizes = {
        "Sapling": (0.72, 0.28),
        "Young": (1.18, 0.46),
        "Mature": (1.72, 0.68),
    }
    height, radius = sizes[recipe.variant]
    add_tree("Tree", height, radius, materials)
    join_meshes("TreeMesh")


HUMANOID_BONE_COORDS: dict[str, tuple[tuple[float, float, float], tuple[float, float, float]]] = {
    "Root": ((0.0, 0.0, 0.0), (0.0, 0.0, 0.12)),
    "Pelvis": ((0.0, 0.0, 0.72), (0.0, 0.0, 0.88)),
    "Spine": ((0.0, 0.0, 0.88), (0.0, 0.0, 1.08)),
    "Chest": ((0.0, 0.0, 1.08), (0.0, 0.0, 1.28)),
    "Neck": ((0.0, 0.0, 1.28), (0.0, 0.0, 1.38)),
    "Head": ((0.0, 0.0, 1.38), (0.0, 0.0, 1.62)),
    "UpperArm.L": ((-0.18, 0.0, 1.24), (-0.43, 0.0, 1.10)),
    "LowerArm.L": ((-0.43, 0.0, 1.10), (-0.61, 0.0, 0.91)),
    "Hand.L": ((-0.61, 0.0, 0.91), (-0.68, 0.0, 0.82)),
    "UpperArm.R": ((0.18, 0.0, 1.24), (0.43, 0.0, 1.10)),
    "LowerArm.R": ((0.43, 0.0, 1.10), (0.61, 0.0, 0.91)),
    "Hand.R": ((0.61, 0.0, 0.91), (0.68, 0.0, 0.82)),
    "Thigh.L": ((-0.12, 0.0, 0.75), (-0.12, 0.0, 0.43)),
    "Shin.L": ((-0.12, 0.0, 0.43), (-0.12, 0.0, 0.14)),
    "Foot.L": ((-0.12, 0.0, 0.14), (-0.12, 0.18, 0.07)),
    "Thigh.R": ((0.12, 0.0, 0.75), (0.12, 0.0, 0.43)),
    "Shin.R": ((0.12, 0.0, 0.43), (0.12, 0.0, 0.14)),
    "Foot.R": ((0.12, 0.0, 0.14), (0.12, 0.18, 0.07)),
}


def build_humanoid_armature() -> bpy.types.Object:
    armature_data = bpy.data.armatures.new("HumanoidSkeleton")
    armature = bpy.data.objects.new("Armature", armature_data)
    bpy.context.collection.objects.link(armature)
    bpy.context.view_layer.objects.active = armature
    armature.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    edit_bones: dict[str, bpy.types.EditBone] = {}
    for name, parent in recipes.SKELETON_BONES:
        head, tail = HUMANOID_BONE_COORDS[name]
        bone = armature_data.edit_bones.new(name)
        bone.head = head
        bone.tail = tail
        bone.use_deform = True
        if parent is not None:
            bone.parent = edit_bones[parent]
            bone.use_connect = False
        edit_bones[name] = bone
    bpy.ops.object.mode_set(mode="OBJECT")
    armature.select_set(False)
    return armature


def weight_object_to_bone(obj: bpy.types.Object, bone_name: str) -> None:
    group = obj.vertex_groups.new(name=bone_name)
    group.add(list(range(len(obj.data.vertices))), 1.0, "REPLACE")


def add_weighted_box(
    name: str,
    dimensions: tuple[float, float, float],
    location: tuple[float, float, float],
    material: bpy.types.Material,
    bone: str,
) -> bpy.types.Object:
    obj = add_box(name, dimensions, location, material)
    weight_object_to_bone(obj, bone)
    return obj


def add_weighted_segment(
    name: str,
    bone: str,
    radius: float,
    material: bpy.types.Material,
) -> bpy.types.Object:
    head, tail = HUMANOID_BONE_COORDS[bone]
    obj = add_segment(name, head, tail, radius, material, vertices=8)
    weight_object_to_bone(obj, bone)
    return obj


def add_weighted_sphere(
    name: str,
    radius: float,
    location: tuple[float, float, float],
    material: bpy.types.Material,
    bone: str,
    scale: tuple[float, float, float] = (1.0, 1.0, 1.0),
) -> bpy.types.Object:
    obj = add_uv_sphere(name, radius, location, material, scale)
    weight_object_to_bone(obj, bone)
    return obj


def reset_pose(armature: bpy.types.Object) -> None:
    for pose_bone in armature.pose.bones:
        pose_bone.rotation_mode = "XYZ"
        pose_bone.location = (0.0, 0.0, 0.0)
        pose_bone.rotation_euler = (0.0, 0.0, 0.0)
        pose_bone.scale = (1.0, 1.0, 1.0)


def key_pose(
    armature: bpy.types.Object,
    frame: int,
    rotations: dict[str, tuple[float, float, float]],
) -> None:
    reset_pose(armature)
    for bone_name, rotation in rotations.items():
        armature.pose.bones[bone_name].rotation_euler = rotation
    root = armature.pose.bones["Root"]
    root.keyframe_insert(data_path="location", frame=frame, group="Root")
    for bone_name in sorted(rotations):
        armature.pose.bones[bone_name].keyframe_insert(
            data_path="rotation_euler", frame=frame, group=bone_name
        )


def action_poses(name: str) -> tuple[dict[str, tuple[float, float, float]], ...]:
    neutral: dict[str, tuple[float, float, float]] = {}
    walk_a = {
        "UpperArm.L": (0.46, 0.0, 0.0),
        "UpperArm.R": (-0.46, 0.0, 0.0),
        "Thigh.L": (-0.50, 0.0, 0.0),
        "Thigh.R": (0.50, 0.0, 0.0),
        "Shin.L": (0.18, 0.0, 0.0),
        "Shin.R": (0.42, 0.0, 0.0),
    }
    walk_b = {bone: (-value[0], value[1], value[2]) for bone, value in walk_a.items()}
    carry = {
        "UpperArm.L": (-0.62, 0.0, -0.18),
        "UpperArm.R": (-0.62, 0.0, 0.18),
        "LowerArm.L": (-0.72, 0.0, 0.0),
        "LowerArm.R": (-0.72, 0.0, 0.0),
    }
    wheelbarrow = {
        "UpperArm.L": (-0.80, 0.0, -0.12),
        "UpperArm.R": (-0.80, 0.0, 0.12),
        "LowerArm.L": (-0.26, 0.0, 0.0),
        "LowerArm.R": (-0.26, 0.0, 0.0),
        "Chest": (0.10, 0.0, 0.0),
    }
    if name == "idle":
        return (neutral, {"Chest": (0.025, 0.0, 0.0)}, neutral)
    if name == "walk":
        return (walk_a, walk_b, walk_a)
    if name == "gather":
        return (
            {"Chest": (0.20, 0.0, 0.0), "UpperArm.L": (-0.85, 0.0, -0.12), "UpperArm.R": (-0.70, 0.0, 0.20)},
            {"Chest": (0.42, 0.0, 0.0), "UpperArm.L": (-1.20, 0.0, -0.12), "UpperArm.R": (-1.08, 0.0, 0.20)},
            {"Chest": (0.20, 0.0, 0.0), "UpperArm.L": (-0.85, 0.0, -0.12), "UpperArm.R": (-0.70, 0.0, 0.20)},
        )
    if name == "saw":
        return (
            {"UpperArm.L": (-0.55, 0.0, -0.35), "UpperArm.R": (-0.55, 0.0, 0.35), "LowerArm.L": (-0.70, 0.0, 0.0), "LowerArm.R": (-0.70, 0.0, 0.0)},
            {"UpperArm.L": (-0.95, 0.0, -0.15), "UpperArm.R": (-0.95, 0.0, 0.15), "LowerArm.L": (-0.30, 0.0, 0.0), "LowerArm.R": (-0.30, 0.0, 0.0)},
            {"UpperArm.L": (-0.55, 0.0, -0.35), "UpperArm.R": (-0.55, 0.0, 0.35), "LowerArm.L": (-0.70, 0.0, 0.0), "LowerArm.R": (-0.70, 0.0, 0.0)},
        )
    if name == "stonecut":
        return (
            {"UpperArm.R": (-1.55, 0.0, 0.18), "LowerArm.R": (-0.55, 0.0, 0.0), "UpperArm.L": (-0.65, 0.0, -0.25)},
            {"UpperArm.R": (-0.35, 0.0, 0.18), "LowerArm.R": (-1.10, 0.0, 0.0), "UpperArm.L": (-0.75, 0.0, -0.25)},
            {"UpperArm.R": (-1.55, 0.0, 0.18), "LowerArm.R": (-0.55, 0.0, 0.0), "UpperArm.L": (-0.65, 0.0, -0.25)},
        )
    if name == "cook":
        return (
            {"UpperArm.R": (-0.78, 0.0, 0.28), "LowerArm.R": (-0.82, 0.0, 0.0), "UpperArm.L": (-0.52, 0.0, -0.22)},
            {"UpperArm.R": (-0.62, 0.0, -0.28), "LowerArm.R": (-0.55, 0.0, 0.0), "UpperArm.L": (-0.52, 0.0, -0.22)},
            {"UpperArm.R": (-0.78, 0.0, 0.28), "LowerArm.R": (-0.82, 0.0, 0.0), "UpperArm.L": (-0.52, 0.0, -0.22)},
        )
    if name == "carry_idle":
        return (carry, {**carry, "Chest": (0.03, 0.0, 0.0)}, carry)
    if name == "carry_walk":
        return (
            {**carry, "Thigh.L": (-0.45, 0.0, 0.0), "Thigh.R": (0.45, 0.0, 0.0)},
            {**carry, "Thigh.L": (0.45, 0.0, 0.0), "Thigh.R": (-0.45, 0.0, 0.0)},
            {**carry, "Thigh.L": (-0.45, 0.0, 0.0), "Thigh.R": (0.45, 0.0, 0.0)},
        )
    if name == "wheelbarrow_idle":
        return (wheelbarrow, {**wheelbarrow, "Chest": (0.13, 0.0, 0.0)}, wheelbarrow)
    if name == "wheelbarrow_walk":
        return (
            {**wheelbarrow, "Thigh.L": (-0.44, 0.0, 0.0), "Thigh.R": (0.44, 0.0, 0.0)},
            {**wheelbarrow, "Thigh.L": (0.44, 0.0, 0.0), "Thigh.R": (-0.44, 0.0, 0.0)},
            {**wheelbarrow, "Thigh.L": (-0.44, 0.0, 0.0), "Thigh.R": (0.44, 0.0, 0.0)},
        )
    raise AssertionError(f"unhandled animation {name}")


def create_humanoid_actions(armature: bpy.types.Object) -> None:
    armature.animation_data_create()
    for name in recipes.NPC_ANIMATIONS:
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        armature.animation_data.action = action
        for frame, pose in zip((0, 15, 30), action_poses(name), strict=True):
            key_pose(armature, frame, pose)
    armature.animation_data.action = None
    reset_pose(armature)


def build_npc(recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]) -> None:
    armature = build_humanoid_armature()
    character = materials["character"]
    structure = materials["structure"]

    add_weighted_box("PelvisMesh", (0.38, 0.25, 0.20), (0.0, 0.0, 0.79), character, "Pelvis")
    add_weighted_box("TorsoMesh", (0.46, 0.28, 0.48), (0.0, 0.0, 1.08), character, "Chest")
    add_weighted_sphere("HeadMesh", 0.18, (0.0, 0.0, 1.52), character, "Head", (0.92, 0.88, 1.10))
    for side in ("L", "R"):
        add_weighted_segment(f"UpperArmMesh.{side}", f"UpperArm.{side}", 0.075, character)
        add_weighted_segment(f"LowerArmMesh.{side}", f"LowerArm.{side}", 0.065, character)
        hand_head, hand_tail = HUMANOID_BONE_COORDS[f"Hand.{side}"]
        hand_center = tuple((Vector(hand_head) + Vector(hand_tail)) * 0.5)
        add_weighted_sphere(f"HandMesh.{side}", 0.075, hand_center, character, f"Hand.{side}", (0.75, 0.65, 1.0))
        add_weighted_segment(f"ThighMesh.{side}", f"Thigh.{side}", 0.095, character)
        add_weighted_segment(f"ShinMesh.{side}", f"Shin.{side}", 0.080, character)
        foot_head, foot_tail = HUMANOID_BONE_COORDS[f"Foot.{side}"]
        foot_center = tuple((Vector(foot_head) + Vector(foot_tail)) * 0.5)
        add_weighted_box(f"FootMesh.{side}", (0.16, 0.28, 0.11), foot_center, structure, f"Foot.{side}")

    # Appearance-specific rigid pieces preserve the canonical skin and hierarchy.
    if recipe.variant == "Colonist":
        add_weighted_box("ChestBadge", (0.12, 0.035, 0.12), (0.12, 0.155, 1.14), structure, "Chest")
    elif recipe.variant == "Engineer":
        add_weighted_box("ToolPack", (0.34, 0.16, 0.38), (0.0, -0.20, 1.08), structure, "Chest")
        add_weighted_box("Visor", (0.30, 0.06, 0.10), (0.0, 0.18, 1.55), structure, "Head")
    elif recipe.variant == "Botanist":
        add_weighted_crown = add_weighted_sphere(
            "BotanistCap", 0.20, (0.0, 0.0, 1.65), structure, "Head", (1.10, 1.10, 0.45)
        )
        add_weighted_crown.name = "BotanistCap"
    elif recipe.variant == "Miner":
        helmet = add_weighted_sphere("MinerHelmet", 0.205, (0.0, 0.0, 1.56), structure, "Head", (1.05, 1.02, 0.80))
        helmet.name = "MinerHelmet"
        add_weighted_box("HelmetLamp", (0.10, 0.07, 0.08), (0.0, 0.205, 1.61), structure, "Head")
    elif recipe.variant == "Scout":
        add_weighted_box("ScoutPack", (0.28, 0.14, 0.42), (0.0, -0.18, 1.06), structure, "Chest")
        add_weighted_box("ScoutVisor", (0.32, 0.055, 0.09), (0.0, 0.19, 1.55), structure, "Head")
    else:
        raise AssertionError(f"unhandled NPC appearance {recipe.variant}")

    mesh = join_meshes("CharacterMesh")
    mesh.parent = armature
    modifier = mesh.modifiers.new("HumanoidArmature", "ARMATURE")
    modifier.object = armature
    armature["boapspace_forward"] = "-Z"
    create_humanoid_actions(armature)


def create_object_action(
    obj: bpy.types.Object,
    name: str,
    keys: Sequence[tuple[int, tuple[float, float, float], tuple[float, float, float]]],
) -> bpy.types.Action:
    obj.animation_data_create()
    action = bpy.data.actions.new(name)
    action.use_fake_user = True
    obj.animation_data.action = action
    for frame, location, rotation in keys:
        obj.location = location
        obj.rotation_mode = "XYZ"
        obj.rotation_euler = rotation
        obj.keyframe_insert(data_path="location", frame=frame, group="Root")
        obj.keyframe_insert(data_path="rotation_euler", frame=frame, group="Root")
    obj.animation_data.action = None
    obj.location = (0.0, 0.0, 0.0)
    obj.rotation_euler = (0.0, 0.0, 0.0)
    track = obj.animation_data.nla_tracks.new()
    track.name = name
    track.strips.new(name, int(keys[0][0]), action)
    return action


def build_wheelbarrow(
    _recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]
) -> None:
    root = bpy.data.objects.new("WheelbarrowRoot", None)
    bpy.context.collection.objects.link(root)
    meshes: list[bpy.types.Object] = []
    meshes.append(add_box("Tray", (0.92, 1.02, 0.24), (0.0, -0.15, 0.55), materials["structure"], rotation=(-0.12, 0.0, 0.0)))
    meshes.append(add_cylinder("Wheel", 0.31, 0.14, (0.0, 0.48, 0.31), materials["structure"], vertices=14, rotation=(0.0, math.pi * 0.5, 0.0)))
    for side, x in (("L", -0.24), ("R", 0.24)):
        meshes.append(add_segment(f"Handle{side}", (x, -0.22, 0.52), (x, -0.92, 0.76), 0.045, materials["timber"], vertices=8))
        meshes.append(add_segment(f"Leg{side}", (x, -0.34, 0.47), (x, -0.56, 0.08), 0.04, materials["timber"], vertices=8))
    for mesh in meshes:
        world = mesh.matrix_world.copy()
        mesh.parent = root
        mesh.matrix_world = world
    root["boapspace_forward"] = "-Z"
    create_object_action(
        root,
        "idle",
        (
            (0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
            (15, (0.0, 0.0, 0.008), (0.0, 0.0, 0.0)),
            (30, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)),
        ),
    )
    create_object_action(
        root,
        "roll",
        (
            (0, (0.0, 0.0, 0.0), (0.0, 0.0, -0.015)),
            (8, (0.0, 0.0, 0.018), (0.0, 0.0, 0.015)),
            (15, (0.0, 0.0, 0.0), (0.0, 0.0, -0.015)),
            (23, (0.0, 0.0, 0.018), (0.0, 0.0, 0.015)),
            (30, (0.0, 0.0, 0.0), (0.0, 0.0, -0.015)),
        ),
    )


def build_work_props(
    _recipe: recipes.ModelRecipe, materials: dict[str, bpy.types.Material]
) -> None:
    # Four compact named sub-libraries remain separate GLB nodes so wrappers can
    # choose a prop without stringly creating geometry at runtime.
    add_segment("GatherToolHandle", (-0.80, -0.48, 0.10), (-0.80, -0.48, 0.82), 0.035, materials["timber"], vertices=8)
    add_box("GatherToolHead", (0.34, 0.08, 0.12), (-0.80, -0.48, 0.78), materials["structure"])
    add_box("SawBlade", (0.58, 0.055, 0.16), (0.0, -0.48, 0.35), materials["structure"], rotation=(0.0, 0.12, 0.0))
    add_box("SawHandle", (0.18, 0.10, 0.24), (-0.35, -0.48, 0.38), materials["timber"])
    add_segment("StonecutHammerHandle", (0.56, 0.20, 0.10), (0.56, 0.20, 0.68), 0.035, materials["timber"], vertices=8)
    add_box("StonecutHammerHead", (0.32, 0.16, 0.16), (0.56, 0.20, 0.68), materials["masonry_ore"])
    add_segment("StonecutChisel", (0.86, 0.18, 0.10), (0.86, 0.18, 0.56), 0.035, materials["structure"], vertices=8)
    add_segment("CookLadleHandle", (-0.28, 0.50, 0.10), (-0.28, 0.50, 0.72), 0.025, materials["structure"], vertices=8)
    add_uv_sphere("CookLadleBowl", 0.10, (-0.28, 0.50, 0.10), materials["structure"], (1.0, 0.55, 0.35))


BUILDERS = {
    "building": build_building,
    "resource": build_resource,
    "crop": build_crop,
    "tree": build_tree,
    "npc": build_npc,
    "wheelbarrow": build_wheelbarrow,
    "work_props": build_work_props,
}


def scene_triangle_count() -> int:
    count = 0
    dependency_graph = bpy.context.evaluated_depsgraph_get()
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        evaluated = obj.evaluated_get(dependency_graph)
        mesh = evaluated.to_mesh()
        try:
            count += sum(max(0, len(polygon.vertices) - 2) for polygon in mesh.polygons)
        finally:
            evaluated.to_mesh_clear()
    return count


def blender_to_godot(point: Vector) -> tuple[float, float, float]:
    # Blender is +Z up; glTF/Godot use +Y up and canonical model forward -Z.
    return (float(point.x), float(point.z), float(-point.y))


def scene_bounds() -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    points: list[tuple[float, float, float]] = []
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        points.extend(blender_to_godot(obj.matrix_world @ Vector(corner)) for corner in obj.bound_box)
    if not points:
        raise RuntimeError("asset scene has no mesh bounds")
    minimum = tuple(min(point[axis] for point in points) for axis in range(3))
    maximum = tuple(max(point[axis] for point in points) for axis in range(3))
    return minimum, maximum


def validate_scene_contract(
    recipe: recipes.ModelRecipe,
    triangle_count: int,
    bounds_min: tuple[float, float, float],
    bounds_max: tuple[float, float, float],
) -> None:
    if triangle_count <= 0 or triangle_count > recipe.triangle_budget:
        raise RuntimeError(
            f"{recipe.id} has {triangle_count} triangles; budget is {recipe.triangle_budget}"
        )
    values = (*bounds_min, *bounds_max)
    if not all(math.isfinite(value) for value in values):
        raise RuntimeError(f"{recipe.id} has non-finite bounds")
    if bounds_min[1] < -0.03:
        raise RuntimeError(f"{recipe.id} extends below its ground pivot: {bounds_min[1]}")
    allowed_x = recipe.footprint[0] * recipes.TILE_UNITS * 0.5 + 0.05
    allowed_z = recipe.footprint[1] * recipes.TILE_UNITS * 0.5 + 0.05
    if bounds_min[0] < -allowed_x or bounds_max[0] > allowed_x:
        raise RuntimeError(f"{recipe.id} exceeds its {recipe.footprint[0]}-tile X footprint")
    if bounds_min[2] < -allowed_z or bounds_max[2] > allowed_z:
        raise RuntimeError(f"{recipe.id} exceeds its {recipe.footprint[1]}-tile Z footprint")


def provenance_hash(
    recipe: recipes.ModelRecipe,
    approved_sources: dict[str, ApprovedSource],
) -> str:
    digest = hashlib.sha256()
    digest.update(f"Blender {recipes.BLENDER_VERSION}\n".encode())
    digest.update(Path(__file__).read_bytes())
    digest.update((SCRIPT_DIR / "recipes.py").read_bytes())
    digest.update(json.dumps(recipe.__dict__, sort_keys=True, default=str).encode())
    for source_id in sorted(approved_sources):
        digest.update(source_id.encode())
        digest.update(approved_sources[source_id].sha256.encode())
    return digest.hexdigest()


def canonicalize_glb_triangle_order(path: Path) -> None:
    """Remove Blender's nondeterministic polygon iteration from GLB bytes.

    Blender 5.0 emits identical vertices but may vary the order of triangles
    created by BMesh primitives between processes.  Draw order is semantically
    irrelevant for opaque assets; rotating each triangle without changing its
    winding and sorting the triangle list produces a stable index buffer.
    """

    data = bytearray(path.read_bytes())
    if len(data) < 28 or data[:4] != b"glTF":
        raise RuntimeError(f"cannot canonicalize invalid GLB {path}")
    json_length, json_type = struct.unpack_from("<II", data, 12)
    if json_type != 0x4E4F534A:
        raise RuntimeError(f"GLB {path} does not start with a JSON chunk")
    document = json.loads(bytes(data[20 : 20 + json_length]).rstrip(b" \t\r\n\0"))
    binary_header = 20 + json_length
    binary_length, binary_type = struct.unpack_from("<II", data, binary_header)
    if binary_type != 0x004E4942:
        raise RuntimeError(f"GLB {path} has no binary chunk after JSON")
    binary_start = binary_header + 8
    binary_end = binary_start + binary_length
    if binary_end > len(data):
        raise RuntimeError(f"GLB {path} has a truncated binary chunk")

    handled: set[int] = set()
    formats = {5121: ("B", 1), 5123: ("H", 2), 5125: ("I", 4)}
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            if int(primitive.get("mode", 4)) != 4 or "indices" not in primitive:
                continue
            accessor_index = int(primitive["indices"])
            if accessor_index in handled:
                continue
            handled.add(accessor_index)
            accessor = document["accessors"][accessor_index]
            if accessor.get("type") != "SCALAR" or "sparse" in accessor:
                raise RuntimeError(f"unsupported index accessor in {path}")
            component_type = int(accessor["componentType"])
            if component_type not in formats:
                raise RuntimeError(f"unsupported index component type in {path}")
            format_code, component_size = formats[component_type]
            view = document["bufferViews"][accessor["bufferView"]]
            stride = int(view.get("byteStride", component_size))
            if stride != component_size:
                raise RuntimeError(f"strided index accessor is unsupported in {path}")
            count = int(accessor["count"])
            if count % 3:
                raise RuntimeError(f"triangle index count is invalid in {path}")
            offset = (
                binary_start
                + int(view.get("byteOffset", 0))
                + int(accessor.get("byteOffset", 0))
            )
            indices = list(struct.unpack_from("<" + format_code * count, data, offset))
            triangles = []
            for index in range(0, count, 3):
                triangle = tuple(indices[index : index + 3])
                smallest = triangle.index(min(triangle))
                triangles.append(triangle[smallest:] + triangle[:smallest])
            triangles.sort()
            flattened = [index for triangle in triangles for index in triangle]
            struct.pack_into("<" + format_code * count, data, offset, *flattened)
    # Blender's GLB exporter embeds a private copy of every atlas. Point image
    # records at the five tracked shared atlas sets instead, then discard every
    # buffer view no longer reached by an accessor. Godot therefore imports the
    # shared images without extracting per-model PNGs, and the GLBs do not carry
    # dead duplicate image bytes.
    for image in document.get("images", []):
        image_name = image.get("name")
        if not isinstance(image_name, str) or not any(
            image_name == output_name
            for material in recipes.MATERIALS
            for output_name in (
                f"{material.id}_base_color",
                f"{material.id}_normal",
                f"{material.id}_orm",
            )
        ):
            raise RuntimeError(f"GLB {path} contains unknown embedded image {image_name!r}")
        image.pop("bufferView", None)
        image.pop("mimeType", None)
        image["uri"] = f"../materials/{image_name}.png"

    original_views = document.get("bufferViews", [])
    used_view_indices = sorted(
        {
            int(accessor["bufferView"])
            for accessor in document.get("accessors", [])
            if "bufferView" in accessor
        }
    )
    view_remap: dict[int, int] = {}
    pruned_views = []
    pruned_binary = bytearray()
    for old_index in used_view_indices:
        view = dict(original_views[old_index])
        while len(pruned_binary) % 4:
            pruned_binary.append(0)
        old_offset = int(view.get("byteOffset", 0))
        byte_length = int(view["byteLength"])
        view["byteOffset"] = len(pruned_binary)
        pruned_binary.extend(data[binary_start + old_offset : binary_start + old_offset + byte_length])
        view_remap[old_index] = len(pruned_views)
        pruned_views.append(view)
    for accessor in document.get("accessors", []):
        if "bufferView" in accessor:
            accessor["bufferView"] = view_remap[int(accessor["bufferView"])]
    document["bufferViews"] = pruned_views
    document["buffers"][0]["byteLength"] = len(pruned_binary)

    json_bytes = json.dumps(document, ensure_ascii=True, separators=(",", ":")).encode("utf-8")
    json_bytes += b" " * ((-len(json_bytes)) % 4)
    binary_bytes = bytes(pruned_binary)
    binary_bytes += b"\0" * ((-len(binary_bytes)) % 4)
    total_length = 12 + 8 + len(json_bytes) + 8 + len(binary_bytes)
    rebuilt = bytearray(struct.pack("<4sII", b"glTF", 2, total_length))
    rebuilt.extend(struct.pack("<II", len(json_bytes), 0x4E4F534A))
    rebuilt.extend(json_bytes)
    rebuilt.extend(struct.pack("<II", len(binary_bytes), 0x004E4942))
    rebuilt.extend(binary_bytes)
    path.write_bytes(rebuilt)


def export_model(
    recipe: recipes.ModelRecipe,
    output_root: Path,
    approved_sources: dict[str, ApprovedSource],
) -> ModelRecord:
    clean_scene()
    materials = load_materials(output_root)
    BUILDERS[recipe.builder](recipe, materials)
    triangle_count = scene_triangle_count()
    bounds_min, bounds_max = scene_bounds()
    validate_scene_contract(recipe, triangle_count, bounds_min, bounds_max)

    destination = output_root / recipe.path
    destination.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(destination),
        export_format="GLB",
        check_existing=False,
        use_visible=True,
        export_yup=True,
        export_apply=False,
        export_cameras=False,
        export_lights=False,
        export_materials="EXPORT",
        export_image_format="AUTO",
        export_texcoords=True,
        export_normals=True,
        export_tangents=False,
        export_animations=bool(recipe.animations),
        export_animation_mode="NLA_TRACKS" if recipe.family == "wheelbarrow" else "ACTIONS",
        export_frame_range=False,
        export_frame_step=1,
        export_force_sampling=True,
        export_nla_strips=recipe.family == "wheelbarrow",
        export_skins=True,
        export_influence_nb=4,
        export_all_influences=False,
        export_def_bones=True,
        export_leaf_bone=False,
        export_optimize_animation_size=False,
        export_draco_mesh_compression_enable=False,
        export_extras=True,
        export_unused_images=False,
        export_unused_textures=False,
        will_save_settings=False,
    )
    if not destination.is_file():
        raise RuntimeError(f"Blender did not export {destination}")
    canonicalize_glb_triangle_order(destination)
    return ModelRecord(
        recipe=recipe,
        sha256=sha256_file(destination),
        provenance_sha256=provenance_hash(recipe, approved_sources),
        triangle_count=triangle_count,
        bounds_min=bounds_min,
        bounds_max=bounds_max,
    )


def toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=True)


def toml_strings(values: Iterable[str]) -> str:
    return "[" + ", ".join(toml_string(value) for value in values) + "]"


def toml_numbers(values: Iterable[float | int]) -> str:
    formatted = []
    for value in values:
        formatted.append(str(value) if isinstance(value, int) else f"{value:.6f}")
    return "[" + ", ".join(formatted) + "]"


def write_asset_manifest(
    output_root: Path,
    approved_sources: dict[str, ApprovedSource],
    material_records: Sequence[MaterialRecord],
    model_records: Sequence[ModelRecord],
) -> Path:
    lines = [
        f"schema_version = {recipes.SCHEMA_VERSION}",
        f"blender_version = {toml_string(recipes.BLENDER_VERSION)}",
        f"godot_version = {toml_string(recipes.GODOT_VERSION)}",
        f"generator = {toml_string('tools/assets_3d/generate.py')}",
        f"source_manifest = {toml_string(recipes.SOURCE_MANIFEST_PATH.as_posix())}",
        f"tile_units = {recipes.TILE_UNITS:.1f}",
        f"subtile_units_per_tile = {recipes.SUBTILE_UNITS_PER_TILE}",
        'up_axis = "+Y"',
        'forward_axis = "-Z"',
        f"animation_fps = {recipes.ANIMATION_FPS}",
        "ready = true",
        "",
        "[sources]",
    ]
    for source_id in sorted(approved_sources):
        lines.append(f"{source_id}_sha256 = {toml_string(approved_sources[source_id].sha256)}")
    lines.extend(
        [
            "",
            "[skeleton]",
            f"id = {toml_string(recipes.SKELETON_ID)}",
            f"bones = {toml_strings(name for name, _ in recipes.SKELETON_BONES)}",
            "parents = "
            + toml_strings(parent if parent is not None else "" for _, parent in recipes.SKELETON_BONES),
            f"animations = {toml_strings(recipes.NPC_ANIMATIONS)}",
            f"fps = {recipes.ANIMATION_FPS}",
            "stationary_root = true",
            "",
        ]
    )

    for record in material_records:
        material = record.recipe
        lines.extend(
            [
                "[[material]]",
                f"id = {toml_string(material.id)}",
                f"base_color = {toml_string(material.base_color_path.as_posix())}",
                f"normal = {toml_string(material.normal_path.as_posix())}",
                f"orm = {toml_string(material.orm_path.as_posix())}",
                f"base_color_sha256 = {toml_string(record.base_color_sha256)}",
                f"normal_sha256 = {toml_string(record.normal_sha256)}",
                f"orm_sha256 = {toml_string(record.orm_sha256)}",
                "width = 256",
                "height = 256",
                "ready = true",
                "",
            ]
        )

    for record in model_records:
        model = record.recipe
        lines.extend(
            [
                "[[model]]",
                f"id = {toml_string(model.id)}",
                f"family = {toml_string(model.family)}",
                f"variant = {toml_string(model.variant)}",
                f"path = {toml_string(model.path.as_posix())}",
                f"footprint = {toml_numbers(model.footprint)}",
                f"materials = {toml_strings(model.materials)}",
                f"triangle_budget = {model.triangle_budget}",
                f"triangle_count = {record.triangle_count}",
                "pivot = [0.000000, 0.000000, 0.000000]",
                f"bounds_min = {toml_numbers(record.bounds_min)}",
                f"bounds_max = {toml_numbers(record.bounds_max)}",
                f"sha256 = {toml_string(record.sha256)}",
                f"provenance_sha256 = {toml_string(record.provenance_sha256)}",
                f"source_ids = {toml_strings(('model_turnaround', 'material_source'))}",
            ]
        )
        if model.animations:
            lines.append(f"animations = {toml_strings(model.animations)}")
        if model.skeleton is not None:
            lines.append(f"skeleton = {toml_string(model.skeleton)}")
        if model.wrapper is not None:
            lines.append(f"wrapper = {toml_string(model.wrapper.as_posix())}")
        lines.extend(["ready = true", ""])

    destination = output_root / recipes.ASSET_MANIFEST_PATH
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    return destination


def generate_all(source_root: Path, output_root: Path) -> list[Path]:
    recipes.assert_recipe_completeness()
    approved_sources = load_approved_sources(source_root)
    material_records = generate_material_textures(approved_sources["material_source"], output_root)
    model_records = [
        export_model(recipe, output_root, approved_sources) for recipe in recipes.MODELS
    ]
    manifest_path = write_asset_manifest(
        output_root, approved_sources, material_records, model_records
    )
    return [
        *(output_root / record.recipe.base_color_path for record in material_records),
        *(output_root / record.recipe.normal_path for record in material_records),
        *(output_root / record.recipe.orm_path for record in material_records),
        *(output_root / record.recipe.path for record in model_records),
        manifest_path,
    ]


def check_reproducible(source_root: Path, shipping_root: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="boapspace-assets-3d-") as temporary:
        temporary_root = Path(temporary)
        generated = generate_all(source_root, temporary_root)
        differences = []
        for generated_path in generated:
            relative = generated_path.relative_to(temporary_root)
            shipping_path = shipping_root / relative
            if not shipping_path.is_file():
                differences.append(f"missing shipping file {relative}")
            elif generated_path.read_bytes() != shipping_path.read_bytes():
                differences.append(f"non-reproducible output {relative}")
        if differences:
            raise RuntimeError("\n".join(differences))
    print("3D asset regeneration is byte-for-byte reproducible")


def main() -> None:
    arguments = parse_arguments()
    validate_blender_version(arguments.skip_version_check)
    source_root = arguments.source_root.resolve()
    output_root = arguments.output_root.resolve()
    if arguments.check:
        check_reproducible(source_root, output_root)
    else:
        outputs = generate_all(source_root, output_root)
        print(f"generated {len(outputs)} deterministic 3D asset files")


if __name__ == "__main__":
    main()
