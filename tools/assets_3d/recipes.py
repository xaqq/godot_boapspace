"""Declarative, deterministic recipes for Boapspace's experimental 3D assets.

This module deliberately has no Blender dependency.  Both ``generate.py`` and
``validate.py`` import it, which keeps enum coverage, budgets, and paths in one
place and makes accidental generator/validator drift visible.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


BLENDER_VERSION = "5.0.1"
GODOT_VERSION = "4.7.stable.official.5b4e0cb0f"
SCHEMA_VERSION = 1
TILE_UNITS = 2.0
SUBTILE_UNITS_PER_TILE = 1024
ANIMATION_FPS = 30
SKELETON_ID = "boapspace_humanoid_v1"

SOURCE_MANIFEST_PATH = Path("art_sources/world_3d/source_manifest.toml")
ASSET_MANIFEST_PATH = Path("godot/assets/visual/asset_manifest_3d.toml")
TURNAROUND_SOURCE_PATH = Path("art_sources/world_3d/references/model_turnaround.png")
MATERIAL_SOURCE_PATH = Path("art_sources/world_3d/materials/material_source.png")

BUILDING_VARIANTS = (
    "Depot",
    "Warehouse",
    "TownHall",
    "Sawmill",
    "Stoneworks",
    "Kitchen",
    "Farm",
    "Field",
    "ForesterLodge",
    "TreePlot",
    "SmallHouse",
    "MediumHouse",
    "LargeHouse",
)

RESOURCE_VARIANTS = (
    "Wood",
    "Stone",
    "Food",
    "Gold",
    "Crops",
    "WildBerries",
    "Planks",
    "StoneBlocks",
)

CROP_VARIANTS = ("Seedable", "GrowingStep1", "GrowingStep2", "Grown")
TREE_VARIANTS = ("Sapling", "Young", "Mature")
NPC_VARIANTS = ("Colonist", "Engineer", "Botanist", "Miner", "Scout")
NPC_ANIMATIONS = (
    "idle",
    "walk",
    "gather",
    "saw",
    "stonecut",
    "cook",
    "carry_idle",
    "carry_walk",
    "wheelbarrow_idle",
    "wheelbarrow_walk",
)
WHEELBARROW_ANIMATIONS = ("idle", "roll")

# Parent names are part of the shipping contract.  Keep ordering topological:
# it is also the deterministic joint order exported by Blender.
SKELETON_BONES = (
    ("Root", None),
    ("Pelvis", "Root"),
    ("Spine", "Pelvis"),
    ("Chest", "Spine"),
    ("Neck", "Chest"),
    ("Head", "Neck"),
    ("UpperArm.L", "Chest"),
    ("LowerArm.L", "UpperArm.L"),
    ("Hand.L", "LowerArm.L"),
    ("UpperArm.R", "Chest"),
    ("LowerArm.R", "UpperArm.R"),
    ("Hand.R", "LowerArm.R"),
    ("Thigh.L", "Pelvis"),
    ("Shin.L", "Thigh.L"),
    ("Foot.L", "Shin.L"),
    ("Thigh.R", "Pelvis"),
    ("Shin.R", "Thigh.R"),
    ("Foot.R", "Shin.R"),
)


@dataclass(frozen=True)
class MaterialRecipe:
    id: str
    tint: tuple[float, float, float]
    roughness: float
    metallic: float

    @property
    def directory(self) -> Path:
        return Path("godot/assets/visual/world/3d/materials")

    @property
    def base_color_path(self) -> Path:
        return self.directory / f"{self.id}_base_color.png"

    @property
    def normal_path(self) -> Path:
        return self.directory / f"{self.id}_normal.png"

    @property
    def orm_path(self) -> Path:
        return self.directory / f"{self.id}_orm.png"


MATERIALS = (
    MaterialRecipe("structure", (0.83, 0.86, 0.86), 0.56, 0.42),
    MaterialRecipe("timber", (0.68, 0.43, 0.25), 0.78, 0.02),
    MaterialRecipe("masonry_ore", (0.64, 0.65, 0.68), 0.84, 0.08),
    MaterialRecipe("organic", (0.48, 0.67, 0.37), 0.90, 0.00),
    MaterialRecipe("character", (0.76, 0.74, 0.69), 0.62, 0.12),
)


@dataclass(frozen=True)
class ModelRecipe:
    id: str
    family: str
    variant: str
    path: Path
    footprint: tuple[int, int]
    materials: tuple[str, ...]
    triangle_budget: int
    builder: str
    animations: tuple[str, ...] = ()
    skeleton: str | None = None
    wrapper: Path | None = None


def _building(
    slug: str,
    variant: str,
    footprint: tuple[int, int],
    materials: tuple[str, ...],
    budget: int,
) -> ModelRecipe:
    return ModelRecipe(
        id=f"building_{slug}",
        family="building",
        variant=variant,
        path=Path(f"godot/assets/visual/world/3d/buildings/building_{slug}.glb"),
        footprint=footprint,
        materials=materials,
        triangle_budget=budget,
        builder="building",
    )


def _resource(slug: str, variant: str, materials: tuple[str, ...]) -> ModelRecipe:
    return ModelRecipe(
        id=f"resource_{slug}",
        family="resource",
        variant=variant,
        path=Path(f"godot/assets/visual/world/3d/resources/resource_{slug}.glb"),
        footprint=(1, 1),
        materials=materials,
        triangle_budget=1_500,
        builder="resource",
    )


def _farming(slug: str, family: str, variant: str, budget: int) -> ModelRecipe:
    return ModelRecipe(
        id=slug,
        family=family,
        variant=variant,
        path=Path(f"godot/assets/visual/world/3d/farming/{slug}.glb"),
        footprint=(1, 1),
        materials=("organic", "timber"),
        triangle_budget=budget,
        builder=family,
    )


def _npc(slug: str, variant: str) -> ModelRecipe:
    return ModelRecipe(
        id=f"npc_{slug}",
        family="npc",
        variant=variant,
        path=Path(f"godot/assets/visual/world/3d/characters/npc_{slug}.glb"),
        footprint=(1, 1),
        materials=("character", "structure"),
        triangle_budget=8_000,
        builder="npc",
        animations=NPC_ANIMATIONS,
        skeleton=SKELETON_ID,
        wrapper=Path(f"godot/world/3d/npc_{slug}_3d.tscn"),
    )


MODELS = (
    _building("depot", "Depot", (2, 2), ("structure", "timber"), 4_000),
    _building("warehouse", "Warehouse", (4, 4), ("structure", "timber"), 12_000),
    _building("townhall", "TownHall", (3, 3), ("structure", "timber"), 9_000),
    _building("sawmill", "Sawmill", (2, 2), ("timber", "structure"), 5_000),
    _building("stoneworks", "Stoneworks", (2, 2), ("masonry_ore", "structure"), 5_000),
    _building("kitchen", "Kitchen", (2, 2), ("structure", "masonry_ore"), 5_000),
    _building("farm", "Farm", (3, 3), ("timber", "structure"), 8_000),
    _building("field", "Field", (1, 1), ("timber", "organic"), 2_000),
    _building(
        "forester_lodge",
        "ForesterLodge",
        (3, 3),
        ("timber", "structure"),
        8_000,
    ),
    _building("tree_plot", "TreePlot", (1, 1), ("timber", "organic"), 2_000),
    _building("house_small", "SmallHouse", (1, 1), ("structure", "timber"), 3_000),
    _building("house_medium", "MediumHouse", (2, 2), ("structure", "timber"), 5_000),
    _building("house_large", "LargeHouse", (3, 3), ("structure", "timber"), 8_000),
    _resource("wood", "Wood", ("timber",)),
    _resource("stone", "Stone", ("masonry_ore",)),
    _resource("food", "Food", ("organic", "structure")),
    _resource("gold", "Gold", ("masonry_ore", "structure")),
    _resource("crops", "Crops", ("organic",)),
    _resource("wild_berries", "WildBerries", ("organic",)),
    _resource("planks", "Planks", ("timber",)),
    _resource("stone_blocks", "StoneBlocks", ("masonry_ore",)),
    _farming("crop_seedable_plot", "crop", "Seedable", 1_500),
    _farming("crop_growing_step1", "crop", "GrowingStep1", 2_000),
    _farming("crop_growing_step2", "crop", "GrowingStep2", 2_500),
    _farming("crop_grown", "crop", "Grown", 3_000),
    _farming("tree_plot_sapling", "tree", "Sapling", 2_000),
    _farming("tree_plot_young", "tree", "Young", 3_000),
    _farming("tree_plot_mature", "tree", "Mature", 5_000),
    _npc("colonist", "Colonist"),
    _npc("engineer", "Engineer"),
    _npc("botanist", "Botanist"),
    _npc("miner", "Miner"),
    _npc("scout", "Scout"),
    ModelRecipe(
        id="wheelbarrow",
        family="wheelbarrow",
        variant="Wheelbarrow",
        path=Path("godot/assets/visual/world/3d/vehicles/wheelbarrow.glb"),
        footprint=(1, 1),
        materials=("timber", "structure"),
        triangle_budget=4_000,
        builder="wheelbarrow",
        animations=WHEELBARROW_ANIMATIONS,
        wrapper=Path("godot/world/3d/wheelbarrow_3d.tscn"),
    ),
    ModelRecipe(
        id="work_props",
        family="props",
        variant="WorkProps",
        path=Path("godot/assets/visual/world/3d/props/work_props.glb"),
        footprint=(1, 1),
        materials=("timber", "structure", "masonry_ore"),
        triangle_budget=6_000,
        builder="work_props",
    ),
)


MODEL_BY_ID = {model.id: model for model in MODELS}
MATERIAL_BY_ID = {material.id: material for material in MATERIALS}


def assert_recipe_completeness() -> None:
    """Fail early when declarative coverage drifts from simulation enums."""

    assert len(MODELS) == 35
    assert len(MODEL_BY_ID) == len(MODELS)
    assert tuple(model.variant for model in MODELS if model.family == "building") == BUILDING_VARIANTS
    assert tuple(model.variant for model in MODELS if model.family == "resource") == RESOURCE_VARIANTS
    assert tuple(model.variant for model in MODELS if model.family == "crop") == CROP_VARIANTS
    assert tuple(model.variant for model in MODELS if model.family == "tree") == TREE_VARIANTS
    assert tuple(model.variant for model in MODELS if model.family == "npc") == NPC_VARIANTS
    assert {material for model in MODELS for material in model.materials} == set(MATERIAL_BY_ID)


assert_recipe_completeness()
