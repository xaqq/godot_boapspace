use super::world_renderer_3d::ChunkCoord;
use game_engine::buildings::BuildingKind;
use game_engine::components::NpcAppearance;
use game_engine::farming::FieldCropState;
use game_engine::forestry::TreePlotState;
use game_engine::resources::ResourceKind;
use godot::classes::{Mesh, MeshInstance3D, Node, Node3D, PackedScene, ResourceLoader};
use godot::obj::Singleton;
use godot::prelude::*;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

pub(crate) const WHEELBARROW_MODEL_PATH: &str =
    "res://assets/visual/world/3d/vehicles/wheelbarrow.glb";
pub(crate) const WHEELBARROW_WRAPPER_SCENE_PATH: &str = "res://world/3d/wheelbarrow_3d.tscn";
pub(crate) const WORK_PROPS_MODEL_PATH: &str = "res://assets/visual/world/3d/props/work_props.glb";

/// The four crop visuals shipped by the 3D asset pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum CropModel3D {
    Seedable,
    GrowingStep1,
    GrowingStep2,
    Grown,
}

impl CropModel3D {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [
        Self::Seedable,
        Self::GrowingStep1,
        Self::GrowingStep2,
        Self::Grown,
    ];

    /// Inactive fields have no crop overlay. Active seeding intentionally uses
    /// the seedable plot model, matching the established 2D rendering rule.
    pub(crate) const fn from_render_state(state: FieldCropState) -> Option<Self> {
        match state {
            FieldCropState::Inactive => None,
            FieldCropState::Seedable | FieldCropState::Seeding => Some(Self::Seedable),
            FieldCropState::GrowingStep1 => Some(Self::GrowingStep1),
            FieldCropState::GrowingStep2 => Some(Self::GrowingStep2),
            FieldCropState::Grown => Some(Self::Grown),
        }
    }
}

/// The three tree growth visuals rendered above a tree-plot building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TreeModel3D {
    Sapling,
    Young,
    Mature,
}

/// Typed entries in the multi-mesh work-prop source library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum WorkPropModel3D {
    Gather,
    Saw,
    Stonecut,
    Cook,
}

impl WorkPropModel3D {
    pub(crate) const ALL: [Self; 4] = [Self::Gather, Self::Saw, Self::Stonecut, Self::Cook];

    fn from_mesh_node_name(name: &str) -> Option<Self> {
        if name.starts_with("Gather") {
            Some(Self::Gather)
        } else if name.starts_with("Saw") {
            Some(Self::Saw)
        } else if name.starts_with("Stonecut") {
            Some(Self::Stonecut)
        } else if name.starts_with("Cook") {
            Some(Self::Cook)
        } else {
            None
        }
    }
}

impl TreeModel3D {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 3] = [Self::Sapling, Self::Young, Self::Mature];

    pub(crate) const fn from_render_state(state: TreePlotState) -> Option<Self> {
        match state {
            TreePlotState::Inactive | TreePlotState::Seedable | TreePlotState::Seeding => None,
            TreePlotState::Sapling => Some(Self::Sapling),
            TreePlotState::Young => Some(Self::Young),
            TreePlotState::Mature => Some(Self::Mature),
        }
    }
}

/// Stable typed identifier for every model family consumed by the 3D renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelKey3D {
    Building(BuildingKind),
    Resource(ResourceKind),
    Crop(CropModel3D),
    Tree(TreeModel3D),
    Npc(NpcAppearance),
    Wheelbarrow,
    WorkProps,
}

impl ModelKey3D {
    pub(crate) const ALL: [Self; 35] = [
        Self::Building(BuildingKind::Depot),
        Self::Building(BuildingKind::Warehouse),
        Self::Building(BuildingKind::TownHall),
        Self::Building(BuildingKind::Sawmill),
        Self::Building(BuildingKind::Stoneworks),
        Self::Building(BuildingKind::Kitchen),
        Self::Building(BuildingKind::Farm),
        Self::Building(BuildingKind::Field),
        Self::Building(BuildingKind::ForesterLodge),
        Self::Building(BuildingKind::TreePlot),
        Self::Building(BuildingKind::SmallHouse),
        Self::Building(BuildingKind::MediumHouse),
        Self::Building(BuildingKind::LargeHouse),
        Self::Resource(ResourceKind::Wood),
        Self::Resource(ResourceKind::Stone),
        Self::Resource(ResourceKind::Food),
        Self::Resource(ResourceKind::Gold),
        Self::Resource(ResourceKind::Crops),
        Self::Resource(ResourceKind::WildBerries),
        Self::Resource(ResourceKind::Planks),
        Self::Resource(ResourceKind::StoneBlocks),
        Self::Crop(CropModel3D::Seedable),
        Self::Crop(CropModel3D::GrowingStep1),
        Self::Crop(CropModel3D::GrowingStep2),
        Self::Crop(CropModel3D::Grown),
        Self::Tree(TreeModel3D::Sapling),
        Self::Tree(TreeModel3D::Young),
        Self::Tree(TreeModel3D::Mature),
        Self::Npc(NpcAppearance::Colonist),
        Self::Npc(NpcAppearance::Engineer),
        Self::Npc(NpcAppearance::Botanist),
        Self::Npc(NpcAppearance::Miner),
        Self::Npc(NpcAppearance::Scout),
        Self::Wheelbarrow,
        Self::WorkProps,
    ];

    /// The scene instantiated at runtime. Animated models use their typed
    /// wrapper scenes; static models use the imported GLB scene directly.
    pub(crate) const fn scene_path(self) -> &'static str {
        match self {
            Self::Building(kind) => building_model_path(kind),
            Self::Resource(kind) => resource_model_path(kind),
            Self::Crop(model) => crop_model_path(model),
            Self::Tree(model) => tree_model_path(model),
            Self::Npc(appearance) => npc_wrapper_scene_path(appearance),
            Self::Wheelbarrow => WHEELBARROW_WRAPPER_SCENE_PATH,
            Self::WorkProps => WORK_PROPS_MODEL_PATH,
        }
    }

    /// The imported GLB containing source meshes. This differs from
    /// `scene_path` for NPCs and the wheelbarrow, whose runtime scenes add typed
    /// animation and attachment wiring.
    pub(crate) const fn source_glb_path(self) -> &'static str {
        match self {
            Self::Building(kind) => building_model_path(kind),
            Self::Resource(kind) => resource_model_path(kind),
            Self::Crop(model) => crop_model_path(model),
            Self::Tree(model) => tree_model_path(model),
            Self::Npc(appearance) => npc_model_path(appearance),
            Self::Wheelbarrow => WHEELBARROW_MODEL_PATH,
            Self::WorkProps => WORK_PROPS_MODEL_PATH,
        }
    }

    const fn sort_key(self) -> (u8, u8) {
        match self {
            Self::Building(kind) => (0, building_rank(kind)),
            Self::Resource(kind) => (1, resource_rank(kind)),
            Self::Crop(model) => (2, model as u8),
            Self::Tree(model) => (3, model as u8),
            Self::Npc(appearance) => (4, npc_rank(appearance)),
            Self::Wheelbarrow => (5, 0),
            Self::WorkProps => (6, 0),
        }
    }
}

impl From<BuildingKind> for ModelKey3D {
    fn from(value: BuildingKind) -> Self {
        Self::Building(value)
    }
}

impl From<ResourceKind> for ModelKey3D {
    fn from(value: ResourceKind) -> Self {
        Self::Resource(value)
    }
}

impl From<CropModel3D> for ModelKey3D {
    fn from(value: CropModel3D) -> Self {
        Self::Crop(value)
    }
}

impl From<TreeModel3D> for ModelKey3D {
    fn from(value: TreeModel3D) -> Self {
        Self::Tree(value)
    }
}

impl From<NpcAppearance> for ModelKey3D {
    fn from(value: NpcAppearance) -> Self {
        Self::Npc(value)
    }
}

impl Ord for ModelKey3D {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for ModelKey3D {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for ModelKey3D {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.sort_key().hash(state);
    }
}

/// Material override applied to a model batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ModelMaterialState3D {
    Constructed,
    Blueprint,
    PreviewValid,
    PreviewInvalid,
}

impl ModelMaterialState3D {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [
        Self::Constructed,
        Self::Blueprint,
        Self::PreviewValid,
        Self::PreviewInvalid,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ModelLod3D {
    Detailed,
    Overview,
}

impl ModelLod3D {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 2] = [Self::Detailed, Self::Overview];
}

/// Deterministic key for `MultiMesh` grouping and stable renderer diffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ModelGroupKey3D {
    pub(crate) chunk: ChunkCoord,
    pub(crate) model: ModelKey3D,
    pub(crate) material_state: ModelMaterialState3D,
    pub(crate) lod: ModelLod3D,
}

impl ModelGroupKey3D {
    pub(crate) const fn new(
        chunk: ChunkCoord,
        model: ModelKey3D,
        material_state: ModelMaterialState3D,
        lod: ModelLod3D,
    ) -> Self {
        Self {
            chunk,
            model,
            material_state,
            lod,
        }
    }
}

pub(crate) const fn building_model_path(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Depot => "res://assets/visual/world/3d/buildings/building_depot.glb",
        BuildingKind::Warehouse => "res://assets/visual/world/3d/buildings/building_warehouse.glb",
        BuildingKind::TownHall => "res://assets/visual/world/3d/buildings/building_townhall.glb",
        BuildingKind::Sawmill => "res://assets/visual/world/3d/buildings/building_sawmill.glb",
        BuildingKind::Stoneworks => {
            "res://assets/visual/world/3d/buildings/building_stoneworks.glb"
        }
        BuildingKind::Kitchen => "res://assets/visual/world/3d/buildings/building_kitchen.glb",
        BuildingKind::Farm => "res://assets/visual/world/3d/buildings/building_farm.glb",
        BuildingKind::Field => "res://assets/visual/world/3d/buildings/building_field.glb",
        BuildingKind::ForesterLodge => {
            "res://assets/visual/world/3d/buildings/building_forester_lodge.glb"
        }
        BuildingKind::TreePlot => "res://assets/visual/world/3d/buildings/building_tree_plot.glb",
        BuildingKind::SmallHouse => {
            "res://assets/visual/world/3d/buildings/building_house_small.glb"
        }
        BuildingKind::MediumHouse => {
            "res://assets/visual/world/3d/buildings/building_house_medium.glb"
        }
        BuildingKind::LargeHouse => {
            "res://assets/visual/world/3d/buildings/building_house_large.glb"
        }
    }
}

pub(crate) const fn resource_model_path(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "res://assets/visual/world/3d/resources/resource_wood.glb",
        ResourceKind::Stone => "res://assets/visual/world/3d/resources/resource_stone.glb",
        ResourceKind::Food => "res://assets/visual/world/3d/resources/resource_food.glb",
        ResourceKind::Gold => "res://assets/visual/world/3d/resources/resource_gold.glb",
        ResourceKind::Crops => "res://assets/visual/world/3d/resources/resource_crops.glb",
        ResourceKind::WildBerries => {
            "res://assets/visual/world/3d/resources/resource_wild_berries.glb"
        }
        ResourceKind::Planks => "res://assets/visual/world/3d/resources/resource_planks.glb",
        ResourceKind::StoneBlocks => {
            "res://assets/visual/world/3d/resources/resource_stone_blocks.glb"
        }
    }
}

pub(crate) const fn crop_model_path(model: CropModel3D) -> &'static str {
    match model {
        CropModel3D::Seedable => "res://assets/visual/world/3d/farming/crop_seedable_plot.glb",
        CropModel3D::GrowingStep1 => "res://assets/visual/world/3d/farming/crop_growing_step1.glb",
        CropModel3D::GrowingStep2 => "res://assets/visual/world/3d/farming/crop_growing_step2.glb",
        CropModel3D::Grown => "res://assets/visual/world/3d/farming/crop_grown.glb",
    }
}

pub(crate) const fn tree_model_path(model: TreeModel3D) -> &'static str {
    match model {
        TreeModel3D::Sapling => "res://assets/visual/world/3d/farming/tree_plot_sapling.glb",
        TreeModel3D::Young => "res://assets/visual/world/3d/farming/tree_plot_young.glb",
        TreeModel3D::Mature => "res://assets/visual/world/3d/farming/tree_plot_mature.glb",
    }
}

pub(crate) const fn npc_model_path(appearance: NpcAppearance) -> &'static str {
    match appearance {
        NpcAppearance::Colonist => "res://assets/visual/world/3d/characters/npc_colonist.glb",
        NpcAppearance::Engineer => "res://assets/visual/world/3d/characters/npc_engineer.glb",
        NpcAppearance::Botanist => "res://assets/visual/world/3d/characters/npc_botanist.glb",
        NpcAppearance::Miner => "res://assets/visual/world/3d/characters/npc_miner.glb",
        NpcAppearance::Scout => "res://assets/visual/world/3d/characters/npc_scout.glb",
    }
}

pub(crate) const fn npc_wrapper_scene_path(appearance: NpcAppearance) -> &'static str {
    match appearance {
        NpcAppearance::Colonist => "res://world/3d/npc_colonist_3d.tscn",
        NpcAppearance::Engineer => "res://world/3d/npc_engineer_3d.tscn",
        NpcAppearance::Botanist => "res://world/3d/npc_botanist_3d.tscn",
        NpcAppearance::Miner => "res://world/3d/npc_miner_3d.tscn",
        NpcAppearance::Scout => "res://world/3d/npc_scout_3d.tscn",
    }
}

const fn building_rank(kind: BuildingKind) -> u8 {
    match kind {
        BuildingKind::Depot => 0,
        BuildingKind::Warehouse => 1,
        BuildingKind::TownHall => 2,
        BuildingKind::Sawmill => 3,
        BuildingKind::Stoneworks => 4,
        BuildingKind::Kitchen => 5,
        BuildingKind::Farm => 6,
        BuildingKind::Field => 7,
        BuildingKind::ForesterLodge => 8,
        BuildingKind::TreePlot => 9,
        BuildingKind::SmallHouse => 10,
        BuildingKind::MediumHouse => 11,
        BuildingKind::LargeHouse => 12,
    }
}

const fn resource_rank(kind: ResourceKind) -> u8 {
    match kind {
        ResourceKind::Wood => 0,
        ResourceKind::Stone => 1,
        ResourceKind::Food => 2,
        ResourceKind::Gold => 3,
        ResourceKind::Crops => 4,
        ResourceKind::WildBerries => 5,
        ResourceKind::Planks => 6,
        ResourceKind::StoneBlocks => 7,
    }
}

const fn npc_rank(appearance: NpcAppearance) -> u8 {
    match appearance {
        NpcAppearance::Colonist => 0,
        NpcAppearance::Engineer => 1,
        NpcAppearance::Botanist => 2,
        NpcAppearance::Miner => 3,
        NpcAppearance::Scout => 4,
    }
}

/// Loads the scene used to instantiate a typed model. GLBs and wrapper scenes
/// both import as `PackedScene`, so callers do not need stringly type checks.
pub(crate) fn load_model_scene(model: ModelKey3D) -> Option<Gd<PackedScene>> {
    load_packed_scene(model.scene_path(), "3D model scene")
}

/// Loads the imported GLB for a typed model, bypassing an animation wrapper
/// when mesh data is required for batching.
pub(crate) fn load_model_source_scene(model: ModelKey3D) -> Option<Gd<PackedScene>> {
    load_packed_scene(model.source_glb_path(), "3D model source")
}

fn load_packed_scene(path: &str, context: &str) -> Option<Gd<PackedScene>> {
    let Some(resource) = ResourceLoader::singleton()
        .load_ex(path)
        .type_hint("PackedScene")
        .done()
    else {
        godot_error!("{context}: failed to load {path}");
        return None;
    };

    match resource.try_cast::<PackedScene>() {
        Ok(scene) => Some(scene),
        Err(resource) => {
            godot_error!(
                "{context}: loaded {path} as {}, expected PackedScene",
                resource.get_class()
            );
            None
        }
    }
}

/// A source mesh together with the transform authored on its imported GLB
/// node. Godot keeps object translation/rotation outside the mesh resource,
/// so batching the mesh without this transform would move authored geometry.
#[derive(Clone)]
pub(crate) struct ModelMesh3D {
    pub(crate) mesh: Gd<Mesh>,
    pub(crate) source_transform: Transform3D,
}

/// Instantiates a packed scene temporarily, recursively locates its first
/// `MeshInstance3D`, clones both its mesh resource and accumulated source
/// transform, and immediately frees the temporary node hierarchy.
pub(crate) fn clone_first_mesh_from_scene(scene: &Gd<PackedScene>) -> Option<ModelMesh3D> {
    let Some(root) = scene.instantiate() else {
        godot_error!("3D model source: failed to instantiate PackedScene");
        return None;
    };

    let mesh =
        find_first_mesh(&root, Transform3D::IDENTITY).map(|(mesh, source_transform)| ModelMesh3D {
            mesh: mesh.duplicate_resource(),
            source_transform,
        });
    root.free();
    if mesh.is_none() {
        godot_error!("3D model source: instantiated scene contains no MeshInstance3D mesh");
    }
    mesh
}

/// Loads and clones the first source mesh and its authored transform for a
/// typed model.
pub(crate) fn load_model_mesh(model: ModelKey3D) -> Option<ModelMesh3D> {
    let scene = load_model_source_scene(model)?;
    clone_first_mesh_from_scene(&scene)
}

/// Loads every mesh part in the work-prop GLB and converts its naming boundary
/// into a typed library. Runtime rendering never performs node-path lookup.
pub(crate) fn load_work_prop_meshes() -> Option<BTreeMap<WorkPropModel3D, Vec<ModelMesh3D>>> {
    let scene = load_model_source_scene(ModelKey3D::WorkProps)?;
    let Some(root) = scene.instantiate() else {
        godot_error!("3D work-prop source: failed to instantiate PackedScene");
        return None;
    };

    let mut library = BTreeMap::new();
    collect_work_prop_meshes(&root, Transform3D::IDENTITY, &mut library);
    root.free();
    if WorkPropModel3D::ALL
        .iter()
        .any(|kind| library.get(kind).is_none_or(Vec::is_empty))
    {
        godot_error!("3D work-prop source: one or more typed prop groups are missing");
        return None;
    }
    Some(library)
}

fn find_first_mesh(
    node: &Gd<Node>,
    parent_transform: Transform3D,
) -> Option<(Gd<Mesh>, Transform3D)> {
    let source_transform = node
        .clone()
        .try_cast::<Node3D>()
        .map_or(parent_transform, |node| {
            parent_transform * node.get_transform()
        });
    if let Ok(mesh_instance) = node.clone().try_cast::<MeshInstance3D>() {
        if let Some(mesh) = mesh_instance.get_mesh() {
            return Some((mesh, source_transform));
        }
    }

    for index in 0..node.get_child_count() {
        let child = node.get_child(index)?;
        if let Some(mesh) = find_first_mesh(&child, source_transform) {
            return Some(mesh);
        }
    }
    None
}

fn collect_work_prop_meshes(
    node: &Gd<Node>,
    parent_transform: Transform3D,
    library: &mut BTreeMap<WorkPropModel3D, Vec<ModelMesh3D>>,
) {
    let source_transform = node
        .clone()
        .try_cast::<Node3D>()
        .map_or(parent_transform, |node| {
            parent_transform * node.get_transform()
        });
    if let Ok(mesh_instance) = node.clone().try_cast::<MeshInstance3D>() {
        let name = mesh_instance.get_name().to_string();
        if let (Some(kind), Some(mesh)) = (
            WorkPropModel3D::from_mesh_node_name(&name),
            mesh_instance.get_mesh(),
        ) {
            library.entry(kind).or_default().push(ModelMesh3D {
                mesh: mesh.duplicate_resource(),
                source_transform,
            });
        }
    }

    for index in 0..node.get_child_count() {
        if let Some(child) = node.get_child(index) {
            collect_work_prop_meshes(&child, source_transform, library);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn building_paths_exhaustively_match_every_building_kind() {
        assert_eq!(
            BuildingKind::ALL.map(building_model_path),
            [
                "res://assets/visual/world/3d/buildings/building_depot.glb",
                "res://assets/visual/world/3d/buildings/building_warehouse.glb",
                "res://assets/visual/world/3d/buildings/building_townhall.glb",
                "res://assets/visual/world/3d/buildings/building_sawmill.glb",
                "res://assets/visual/world/3d/buildings/building_stoneworks.glb",
                "res://assets/visual/world/3d/buildings/building_kitchen.glb",
                "res://assets/visual/world/3d/buildings/building_farm.glb",
                "res://assets/visual/world/3d/buildings/building_field.glb",
                "res://assets/visual/world/3d/buildings/building_forester_lodge.glb",
                "res://assets/visual/world/3d/buildings/building_tree_plot.glb",
                "res://assets/visual/world/3d/buildings/building_house_small.glb",
                "res://assets/visual/world/3d/buildings/building_house_medium.glb",
                "res://assets/visual/world/3d/buildings/building_house_large.glb",
            ]
        );
    }

    #[test]
    fn resource_paths_exhaustively_match_every_resource_kind() {
        assert_eq!(
            ResourceKind::ALL.map(resource_model_path),
            [
                "res://assets/visual/world/3d/resources/resource_wood.glb",
                "res://assets/visual/world/3d/resources/resource_stone.glb",
                "res://assets/visual/world/3d/resources/resource_food.glb",
                "res://assets/visual/world/3d/resources/resource_gold.glb",
                "res://assets/visual/world/3d/resources/resource_crops.glb",
                "res://assets/visual/world/3d/resources/resource_wild_berries.glb",
                "res://assets/visual/world/3d/resources/resource_planks.glb",
                "res://assets/visual/world/3d/resources/resource_stone_blocks.glb",
            ]
        );
    }

    #[test]
    fn farming_paths_cover_every_rendered_crop_and_tree_model() {
        assert_eq!(
            CropModel3D::ALL.map(crop_model_path),
            [
                "res://assets/visual/world/3d/farming/crop_seedable_plot.glb",
                "res://assets/visual/world/3d/farming/crop_growing_step1.glb",
                "res://assets/visual/world/3d/farming/crop_growing_step2.glb",
                "res://assets/visual/world/3d/farming/crop_grown.glb",
            ]
        );
        assert_eq!(
            TreeModel3D::ALL.map(tree_model_path),
            [
                "res://assets/visual/world/3d/farming/tree_plot_sapling.glb",
                "res://assets/visual/world/3d/farming/tree_plot_young.glb",
                "res://assets/visual/world/3d/farming/tree_plot_mature.glb",
            ]
        );
    }

    #[test]
    fn farming_state_mapping_matches_rendered_overlay_contract() {
        assert_eq!(
            CropModel3D::from_render_state(FieldCropState::Inactive),
            None
        );
        assert_eq!(
            CropModel3D::from_render_state(FieldCropState::Seedable),
            Some(CropModel3D::Seedable)
        );
        assert_eq!(
            CropModel3D::from_render_state(FieldCropState::Seeding),
            Some(CropModel3D::Seedable)
        );
        assert_eq!(
            CropModel3D::from_render_state(FieldCropState::GrowingStep1),
            Some(CropModel3D::GrowingStep1)
        );
        assert_eq!(
            CropModel3D::from_render_state(FieldCropState::GrowingStep2),
            Some(CropModel3D::GrowingStep2)
        );
        assert_eq!(
            CropModel3D::from_render_state(FieldCropState::Grown),
            Some(CropModel3D::Grown)
        );

        assert_eq!(
            TreeModel3D::from_render_state(TreePlotState::Inactive),
            None
        );
        assert_eq!(
            TreeModel3D::from_render_state(TreePlotState::Seedable),
            None
        );
        assert_eq!(TreeModel3D::from_render_state(TreePlotState::Seeding), None);
        assert_eq!(
            TreeModel3D::from_render_state(TreePlotState::Sapling),
            Some(TreeModel3D::Sapling)
        );
        assert_eq!(
            TreeModel3D::from_render_state(TreePlotState::Young),
            Some(TreeModel3D::Young)
        );
        assert_eq!(
            TreeModel3D::from_render_state(TreePlotState::Mature),
            Some(TreeModel3D::Mature)
        );
    }

    #[test]
    fn npc_paths_cover_every_appearance_and_use_typed_wrappers() {
        assert_eq!(
            NpcAppearance::ALL.map(npc_model_path),
            [
                "res://assets/visual/world/3d/characters/npc_colonist.glb",
                "res://assets/visual/world/3d/characters/npc_engineer.glb",
                "res://assets/visual/world/3d/characters/npc_botanist.glb",
                "res://assets/visual/world/3d/characters/npc_miner.glb",
                "res://assets/visual/world/3d/characters/npc_scout.glb",
            ]
        );
        assert_eq!(
            NpcAppearance::ALL.map(npc_wrapper_scene_path),
            [
                "res://world/3d/npc_colonist_3d.tscn",
                "res://world/3d/npc_engineer_3d.tscn",
                "res://world/3d/npc_botanist_3d.tscn",
                "res://world/3d/npc_miner_3d.tscn",
                "res://world/3d/npc_scout_3d.tscn",
            ]
        );
    }

    #[test]
    fn model_keys_cover_all_thirty_five_unique_asset_contracts() {
        assert_eq!(ModelKey3D::ALL.len(), 35);
        assert_eq!(ModelMaterialState3D::ALL.len(), 4);
        assert_eq!(ModelLod3D::ALL.len(), 2);

        let keys = ModelKey3D::ALL.into_iter().collect::<BTreeSet<_>>();
        let source_paths = ModelKey3D::ALL
            .map(ModelKey3D::source_glb_path)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let scene_paths = ModelKey3D::ALL
            .map(ModelKey3D::scene_path)
            .into_iter()
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), 35);
        assert_eq!(source_paths.len(), 35);
        assert_eq!(scene_paths.len(), 35);

        assert_eq!(
            ModelKey3D::Wheelbarrow.source_glb_path(),
            WHEELBARROW_MODEL_PATH
        );
        assert_eq!(
            ModelKey3D::Wheelbarrow.scene_path(),
            WHEELBARROW_WRAPPER_SCENE_PATH
        );
        assert_eq!(ModelKey3D::WorkProps.scene_path(), WORK_PROPS_MODEL_PATH);
    }

    #[test]
    fn grouping_key_orders_chunk_then_model_state_and_lod() {
        let chunk_zero = ChunkCoord { x: 0, y: 0 };
        let chunk_one = ChunkCoord { x: 1, y: 0 };
        let base = ModelGroupKey3D::new(
            chunk_zero,
            ModelKey3D::Building(BuildingKind::Depot),
            ModelMaterialState3D::Constructed,
            ModelLod3D::Detailed,
        );
        let later_model = ModelGroupKey3D::new(
            chunk_zero,
            ModelKey3D::Resource(ResourceKind::Wood),
            ModelMaterialState3D::Constructed,
            ModelLod3D::Detailed,
        );
        let later_state = ModelGroupKey3D::new(
            chunk_zero,
            ModelKey3D::Resource(ResourceKind::Wood),
            ModelMaterialState3D::Blueprint,
            ModelLod3D::Detailed,
        );
        let later_lod = ModelGroupKey3D::new(
            chunk_zero,
            ModelKey3D::Resource(ResourceKind::Wood),
            ModelMaterialState3D::Blueprint,
            ModelLod3D::Overview,
        );
        let later_chunk = ModelGroupKey3D::new(
            chunk_one,
            ModelKey3D::Building(BuildingKind::Depot),
            ModelMaterialState3D::Constructed,
            ModelLod3D::Detailed,
        );

        let mut keys = vec![later_chunk, later_lod, later_state, later_model, base];
        keys.sort();
        assert_eq!(
            keys,
            vec![base, later_model, later_state, later_lod, later_chunk]
        );
    }
}
