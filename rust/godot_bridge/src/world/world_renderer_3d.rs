use crate::assets::{load_texture, road_asset_path, terrain_asset_path};
use crate::world::mesh_builder_3d::{self, RoadRenderState, RoadSurfaceKey};
use crate::world::model_library_3d::{
    load_model_mesh, load_model_scene, load_work_prop_meshes, CropModel3D, ModelGroupKey3D,
    ModelKey3D, ModelLod3D, ModelMaterialState3D, ModelMesh3D, TreeModel3D, WorkPropModel3D,
};
use crate::world::model_wrapper_3d::{NpcModel3D, WheelbarrowModel3D};
use crate::world::render_snapshot::{
    BuildingRenderState, DynamicRenderSnapshot, NpcActivity, NpcRouteOverlay, PlacementValidity,
    SurfaceRenderSnapshot, WorldOverlaySnapshot,
};
use bevy_ecs::prelude::Entity;
use game_engine::buildings::BuildingFootprint;
use game_engine::components::{MovementFacing, NpcAppearance, SubtileOffset, TerrainKind};
use game_engine::grid::CellCoord;
use game_engine::resources::ResourceKind;
use game_engine::roads::RoadTier;
use godot::classes::{
    base_material_3d, mesh, multi_mesh, ArrayMesh, BoxMesh, Camera3D, INode3D, Material, Mesh,
    MeshInstance3D, MultiMesh, MultiMeshInstance3D, Node3D, PackedScene, StandardMaterial3D,
};
use godot::obj::{EngineEnum, NewAlloc, NewGd, OnEditor};
use godot::prelude::*;
#[cfg(test)]
use std::collections::BTreeSet;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) const WORLD_UNITS_PER_TILE: f32 = 2.0;
pub(crate) const CHUNK_SIZE_TILES: i32 = 32;
const INITIAL_YAW_DEGREES: f32 = 45.0;
const INITIAL_ELEVATION_DEGREES: f32 = 55.0;
const MIN_ELEVATION_DEGREES: f32 = 25.0;
const MAX_ELEVATION_DEGREES: f32 = 80.0;
const INITIAL_DISTANCE_TILES: f32 = 24.0;
const MIN_DISTANCE_TILES: f32 = 4.0;
const MAX_DISTANCE_DIAGONAL_FACTOR: f32 = 1.25;
const ORBIT_SENSITIVITY: f32 = 0.006;
const PAN_TILES_PER_SECOND: f32 = 9.375;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Renderer3DAvailability {
    Preparing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProxyKind3D {
    Building,
    Npc,
    Resource,
    RoadBlueprint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PickProxy3D {
    kind: ProxyKind3D,
    entity: Entity,
    bounds: PickAabb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProxyHit3D {
    pub(crate) kind: ProxyKind3D,
    pub(crate) entity: Entity,
    pub(crate) distance: f32,
}

struct RenderedNpc3D {
    appearance: NpcAppearance,
    node: Gd<NpcModel3D>,
    cargo_kind: Option<ResourceKind>,
    cargo_node: Option<Gd<MeshInstance3D>>,
    wheelbarrow_kind: Option<ResourceKind>,
    wheelbarrow: Option<Gd<WheelbarrowModel3D>>,
    wheelbarrow_load: Option<Gd<MeshInstance3D>>,
    work_prop_kind: Option<WorkPropModel3D>,
    work_prop: Option<Gd<Node3D>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OrbitCameraState {
    focus_tiles: Vector2,
    yaw_radians: f32,
    elevation_radians: f32,
    distance_world: f32,
    surface_width: i32,
    surface_height: i32,
}

impl Default for OrbitCameraState {
    fn default() -> Self {
        Self {
            focus_tiles: Vector2::ZERO,
            yaw_radians: INITIAL_YAW_DEGREES.to_radians(),
            elevation_radians: INITIAL_ELEVATION_DEGREES.to_radians(),
            distance_world: INITIAL_DISTANCE_TILES * WORLD_UNITS_PER_TILE,
            surface_width: 1,
            surface_height: 1,
        }
    }
}

impl OrbitCameraState {
    fn configure_surface(&mut self, width: i32, height: i32, reset_focus: bool) {
        self.surface_width = width.max(1);
        self.surface_height = height.max(1);
        if reset_focus {
            self.focus_tiles = Vector2::new(width as f32, height as f32) * 0.5;
        }
        self.clamp();
    }

    fn clamp(&mut self) {
        self.elevation_radians = self.elevation_radians.clamp(
            MIN_ELEVATION_DEGREES.to_radians(),
            MAX_ELEVATION_DEGREES.to_radians(),
        );
        self.focus_tiles.x = self.focus_tiles.x.clamp(0.0, self.surface_width as f32);
        self.focus_tiles.y = self.focus_tiles.y.clamp(0.0, self.surface_height as f32);
        self.distance_world = self.distance_world.clamp(
            MIN_DISTANCE_TILES * WORLD_UNITS_PER_TILE,
            max_camera_distance_world(self.surface_width, self.surface_height),
        );
    }

    fn focus_world(self) -> Vector3 {
        Vector3::new(
            self.focus_tiles.x * WORLD_UNITS_PER_TILE,
            0.0,
            self.focus_tiles.y * WORLD_UNITS_PER_TILE,
        )
    }

    fn camera_world(self) -> Vector3 {
        let horizontal = self.distance_world * self.elevation_radians.cos();
        self.focus_world()
            + Vector3::new(
                self.yaw_radians.sin() * horizontal,
                self.elevation_radians.sin() * self.distance_world,
                self.yaw_radians.cos() * horizontal,
            )
    }

    fn orbit(&mut self, relative: Vector2) {
        self.yaw_radians =
            (self.yaw_radians - relative.x * ORBIT_SENSITIVITY).rem_euclid(std::f32::consts::TAU);
        self.elevation_radians -= relative.y * ORBIT_SENSITIVITY;
        self.clamp();
    }

    fn dolly(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.distance_world /= factor;
            self.clamp();
        }
    }

    fn pan(&mut self, input: Vector2, delta: f64) {
        if input == Vector2::ZERO {
            return;
        }
        let input = input.normalized();
        let forward = Vector2::new(-self.yaw_radians.sin(), -self.yaw_radians.cos());
        let right = Vector2::new(forward.y, -forward.x);
        let world_delta =
            (right * input.x + forward * -input.y) * PAN_TILES_PER_SECOND * delta as f32;
        self.focus_tiles += world_delta;
        self.clamp();
    }
}

#[derive(GodotClass)]
#[class(base = Node3D)]
pub(crate) struct WorldRenderer3D {
    #[export]
    camera: OnEditor<Gd<Camera3D>>,

    #[export]
    terrain_root: OnEditor<Gd<Node3D>>,

    #[export]
    road_root: OnEditor<Gd<Node3D>>,

    #[export]
    static_root: OnEditor<Gd<Node3D>>,

    #[export]
    npc_root: OnEditor<Gd<Node3D>>,

    #[export]
    overlay_root: OnEditor<Gd<Node3D>>,

    availability: Renderer3DAvailability,
    failure_reason: Option<&'static str>,
    prewarm_cursor: usize,
    orbiting: bool,
    camera_state: OrbitCameraState,
    surface_snapshot: Option<SurfaceRenderSnapshot>,
    dynamic_snapshot: DynamicRenderSnapshot,
    overlay_snapshot: WorldOverlaySnapshot,
    terrain_materials: HashMap<TerrainKind, Gd<Material>>,
    road_materials: HashMap<RoadSurfaceKey, Gd<Material>>,
    model_meshes: HashMap<ModelKey3D, ModelMesh3D>,
    model_scenes: HashMap<ModelKey3D, Gd<PackedScene>>,
    work_prop_meshes: BTreeMap<WorkPropModel3D, Vec<ModelMesh3D>>,
    overview_mesh: Option<Gd<Mesh>>,
    override_materials: HashMap<ModelMaterialState3D, Gd<Material>>,
    terrain_chunks: HashMap<mesh_builder_3d::ChunkCoord, Gd<MeshInstance3D>>,
    road_chunks: HashMap<mesh_builder_3d::ChunkCoord, Gd<MeshInstance3D>>,
    static_batches: BTreeMap<ModelGroupKey3D, Gd<MultiMeshInstance3D>>,
    npc_overview_batches: BTreeMap<ModelGroupKey3D, Gd<MultiMeshInstance3D>>,
    npc_nodes: HashMap<Entity, RenderedNpc3D>,
    pick_proxies: HashMap<ChunkCoord, Vec<PickProxy3D>>,
    grid_overlay: Option<Gd<MeshInstance3D>>,
    world_overlay_nodes: Vec<Gd<MeshInstance3D>>,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for WorldRenderer3D {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            camera: OnEditor::default(),
            terrain_root: OnEditor::default(),
            road_root: OnEditor::default(),
            static_root: OnEditor::default(),
            npc_root: OnEditor::default(),
            overlay_root: OnEditor::default(),
            availability: Renderer3DAvailability::Preparing,
            failure_reason: None,
            prewarm_cursor: 0,
            orbiting: false,
            camera_state: OrbitCameraState::default(),
            surface_snapshot: None,
            dynamic_snapshot: DynamicRenderSnapshot::default(),
            overlay_snapshot: WorldOverlaySnapshot::default(),
            terrain_materials: HashMap::new(),
            road_materials: HashMap::new(),
            model_meshes: HashMap::new(),
            model_scenes: HashMap::new(),
            work_prop_meshes: BTreeMap::new(),
            overview_mesh: None,
            override_materials: HashMap::new(),
            terrain_chunks: HashMap::new(),
            road_chunks: HashMap::new(),
            static_batches: BTreeMap::new(),
            npc_overview_batches: BTreeMap::new(),
            npc_nodes: HashMap::new(),
            pick_proxies: HashMap::new(),
            grid_overlay: None,
            world_overlay_nodes: Vec::new(),
            base,
        }
    }
}

impl WorldRenderer3D {
    pub(crate) const fn availability(&self) -> Renderer3DAvailability {
        self.availability
    }

    pub(crate) const fn failure_reason(&self) -> Option<&'static str> {
        self.failure_reason
    }

    pub(crate) fn mark_ready(&mut self) {
        self.availability = Renderer3DAvailability::Ready;
        self.failure_reason = None;
        self.rebuild_surface_geometry();
        self.rebuild_dynamic_geometry();
        self.rebuild_world_overlay();
    }

    pub(crate) fn prewarm_step(&mut self) -> Renderer3DAvailability {
        if self.availability != Renderer3DAvailability::Preparing {
            return self.availability;
        }

        if self.override_materials.is_empty() {
            self.override_materials = build_override_materials();
        }
        if self.overview_mesh.is_none() {
            let mut mesh = BoxMesh::new_gd();
            mesh.set_size(Vector3::ONE);
            self.overview_mesh = Some(mesh.upcast());
        }

        let terrain_count = TerrainKind::ALL.len();
        let road_count = RoadTier::ALL.len() * 2;
        let model_count = ModelKey3D::ALL.len();
        let task_count = terrain_count + road_count + model_count;
        let cursor = self.prewarm_cursor;
        let succeeded = if cursor < terrain_count {
            let kind = TerrainKind::ALL[cursor];
            load_texture(terrain_asset_path(kind), "WorldRenderer3D terrain")
                .map(|texture| {
                    self.terrain_materials
                        .insert(kind, textured_material(texture, Color::WHITE, false));
                })
                .is_some()
        } else if cursor < terrain_count + road_count {
            let road_index = cursor - terrain_count;
            let tier = RoadTier::ALL[road_index / 2];
            let state = if road_index % 2 == 0 {
                RoadRenderState::Completed
            } else {
                RoadRenderState::Planned
            };
            load_texture(road_asset_path(tier), "WorldRenderer3D road")
                .map(|texture| {
                    let tint = match state {
                        RoadRenderState::Completed => Color::WHITE,
                        RoadRenderState::Planned => Color::from_rgba(0.15, 0.85, 1.0, 0.72),
                    };
                    self.road_materials.insert(
                        RoadSurfaceKey { tier, state },
                        textured_material(texture, tint, state == RoadRenderState::Planned),
                    );
                })
                .is_some()
        } else {
            let model = ModelKey3D::ALL[cursor - terrain_count - road_count];
            if model == ModelKey3D::WorkProps {
                let Some(library) = load_work_prop_meshes() else {
                    self.mark_failed("3D renderer preparation failed while loading work props.");
                    return self.availability;
                };
                self.work_prop_meshes = library;
                self.prewarm_cursor += 1;
                if self.prewarm_cursor >= task_count {
                    self.mark_ready();
                }
                return self.availability;
            }
            let mesh = load_model_mesh(model);
            let scene = match model {
                ModelKey3D::Npc(_) | ModelKey3D::Wheelbarrow => load_model_scene(model),
                _ => None,
            };
            let scene_ok =
                !matches!(model, ModelKey3D::Npc(_) | ModelKey3D::Wheelbarrow) || scene.is_some();
            if let Some(mesh) = mesh {
                self.model_meshes.insert(model, mesh);
            }
            if let Some(scene) = scene {
                self.model_scenes.insert(model, scene);
            }
            self.model_meshes.contains_key(&model) && scene_ok
        };

        if !succeeded {
            self.mark_failed("3D renderer preparation failed while loading an asset.");
            return self.availability;
        }

        self.prewarm_cursor += 1;
        if self.prewarm_cursor >= task_count {
            self.mark_ready();
        }
        self.availability
    }

    pub(crate) fn apply_surface_snapshot(&mut self, snapshot: &SurfaceRenderSnapshot) {
        if self.surface_snapshot.as_ref() == Some(snapshot) {
            return;
        }
        self.surface_snapshot = Some(snapshot.clone());
        if self.availability == Renderer3DAvailability::Ready {
            self.rebuild_surface_geometry();
        }
    }

    pub(crate) fn apply_dynamic_snapshot(&mut self, snapshot: &DynamicRenderSnapshot) {
        if self.dynamic_snapshot == *snapshot {
            return;
        }
        let affected_road_chunks =
            mesh_builder_3d::affected_road_chunks(&self.dynamic_snapshot, snapshot);
        let static_changed = static_visuals_changed(&self.dynamic_snapshot, snapshot);
        let npcs_changed = self.dynamic_snapshot.npcs != snapshot.npcs;
        let proxies_changed = static_changed
            || npcs_changed
            || self.dynamic_snapshot.planned_roads != snapshot.planned_roads;
        self.dynamic_snapshot = snapshot.clone();
        if self.availability == Renderer3DAvailability::Ready {
            self.rebuild_road_chunks(&affected_road_chunks);
            if static_changed {
                self.rebuild_static_batches();
            }
            if npcs_changed {
                self.rebuild_npc_overview_batches();
                self.sync_npc_models();
            }
            if proxies_changed {
                self.rebuild_pick_proxies();
            }
        }
    }

    pub(crate) fn apply_overlay_snapshot(&mut self, snapshot: &WorldOverlaySnapshot) {
        if self.overlay_snapshot == *snapshot {
            return;
        }
        self.overlay_snapshot = snapshot.clone();
        if self.availability == Renderer3DAvailability::Ready {
            self.rebuild_world_overlay();
        }
    }

    fn rebuild_surface_geometry(&mut self) {
        for (_, mut node) in self.terrain_chunks.drain() {
            node.queue_free();
        }
        if let Some(mut grid) = self.grid_overlay.take() {
            grid.queue_free();
        }
        let Some(snapshot) = self.surface_snapshot.as_ref() else {
            return;
        };
        let materials = &self.terrain_materials;
        let mut root = self.terrain_root.clone();
        let mut chunks = Vec::new();
        for chunk in mesh_builder_3d::terrain_chunk_coords(snapshot) {
            let Some(built) = mesh_builder_3d::build_terrain_chunk_mesh(snapshot, chunk, |kind| {
                materials.get(&kind).cloned()
            }) else {
                continue;
            };
            let mut node = MeshInstance3D::new_alloc();
            let mesh: Gd<Mesh> = built.mesh.upcast();
            node.set_mesh(&mesh);
            node.set_position(built.world_origin);
            root.add_child(&node);
            chunks.push((built.chunk, node));
        }
        self.terrain_chunks.extend(chunks);
        self.grid_overlay = build_grid_overlay(snapshot).map(|node| {
            self.overlay_root.add_child(&node);
            node
        });
    }

    fn rebuild_road_geometry(&mut self) {
        for (_, mut node) in self.road_chunks.drain() {
            node.queue_free();
        }
        let chunks = mesh_builder_3d::road_chunk_coords(&self.dynamic_snapshot);
        self.rebuild_road_chunks(&chunks);
    }

    fn rebuild_road_chunks(&mut self, chunks: &[mesh_builder_3d::ChunkCoord]) {
        let materials = &self.road_materials;
        let mut root = self.road_root.clone();
        for chunk in chunks.iter().copied() {
            if let Some(mut old_node) = self.road_chunks.remove(&chunk) {
                old_node.queue_free();
            }
            let Some(built) =
                mesh_builder_3d::build_road_chunk_mesh(&self.dynamic_snapshot, chunk, |key| {
                    materials.get(&key).cloned()
                })
            else {
                continue;
            };
            let mut node = MeshInstance3D::new_alloc();
            let mesh: Gd<Mesh> = built.mesh.upcast();
            node.set_mesh(&mesh);
            node.set_position(built.world_origin);
            root.add_child(&node);
            self.road_chunks.insert(built.chunk, node);
        }
    }

    fn rebuild_dynamic_geometry(&mut self) {
        self.rebuild_road_geometry();
        self.rebuild_static_batches();
        self.rebuild_npc_overview_batches();
        self.sync_npc_models();
        self.rebuild_pick_proxies();
    }

    fn rebuild_static_batches(&mut self) {
        for (_, mut node) in std::mem::take(&mut self.static_batches) {
            node.queue_free();
        }

        let mut groups: BTreeMap<ModelGroupKey3D, Vec<Transform3D>> = BTreeMap::new();
        for building in &self.dynamic_snapshot.buildings {
            let model = ModelKey3D::Building(building.kind);
            let material_state = match building.state {
                BuildingRenderState::Blueprint => ModelMaterialState3D::Blueprint,
                BuildingRenderState::Constructed => ModelMaterialState3D::Constructed,
            };
            let center = footprint_center_3d(building.footprint, 0.0);
            push_model_instance(
                &mut groups,
                building.footprint.origin(),
                model,
                material_state,
                Transform3D::new(Basis::IDENTITY, center),
                overview_transform_for_footprint(building.footprint),
            );
        }
        for resource in &self.dynamic_snapshot.resources {
            let center = cell_center_3d(resource.coord) + Vector3::new(0.0, 0.05, 0.0);
            push_model_instance(
                &mut groups,
                resource.coord,
                ModelKey3D::Resource(resource.kind),
                ModelMaterialState3D::Constructed,
                Transform3D::new(Basis::IDENTITY, center),
                overview_transform(resource.coord, Vector3::new(1.2, 0.75, 1.2)),
            );
        }
        for crop in &self.dynamic_snapshot.crops {
            let Some(model) = CropModel3D::from_render_state(crop.state) else {
                continue;
            };
            let center = cell_center_3d(crop.coord) + Vector3::new(0.0, 0.04, 0.0);
            push_model_instance(
                &mut groups,
                crop.coord,
                ModelKey3D::Crop(model),
                ModelMaterialState3D::Constructed,
                Transform3D::new(Basis::IDENTITY, center),
                overview_transform(crop.coord, Vector3::new(1.4, 0.5, 1.4)),
            );
        }
        for tree in &self.dynamic_snapshot.tree_plots {
            let Some(model) = TreeModel3D::from_render_state(tree.state) else {
                continue;
            };
            let center = cell_center_3d(tree.coord) + Vector3::new(0.0, 0.04, 0.0);
            push_model_instance(
                &mut groups,
                tree.coord,
                ModelKey3D::Tree(model),
                ModelMaterialState3D::Constructed,
                Transform3D::new(Basis::IDENTITY, center),
                overview_transform(tree.coord, Vector3::new(1.3, 2.2, 1.3)),
            );
        }
        let mut root = self.static_root.clone();
        self.static_batches = build_model_batches(
            groups,
            &self.model_meshes,
            self.overview_mesh.as_ref(),
            &self.override_materials,
            &mut root,
        );
    }

    fn rebuild_npc_overview_batches(&mut self) {
        for (_, mut node) in std::mem::take(&mut self.npc_overview_batches) {
            node.queue_free();
        }
        let mut groups: BTreeMap<ModelGroupKey3D, Vec<Transform3D>> = BTreeMap::new();
        for npc in &self.dynamic_snapshot.npcs {
            let position = npc_position_3d(npc.position.coord, npc.position.subtile_offset);
            let key = ModelGroupKey3D::new(
                chunk_coord(npc.position.coord),
                ModelKey3D::Npc(npc.appearance),
                ModelMaterialState3D::Constructed,
                ModelLod3D::Overview,
            );
            groups.entry(key).or_default().push(Transform3D::new(
                Basis::IDENTITY.scaled(Vector3::new(0.55, 1.7, 0.55)),
                position + Vector3::new(0.0, 0.85, 0.0),
            ));
        }
        let mut root = self.static_root.clone();
        self.npc_overview_batches = build_model_batches(
            groups,
            &self.model_meshes,
            self.overview_mesh.as_ref(),
            &self.override_materials,
            &mut root,
        );
    }

    fn sync_npc_models(&mut self) {
        let npcs = self.dynamic_snapshot.npcs.clone();
        let active = npcs.iter().map(|npc| npc.entity).collect::<HashSet<_>>();
        let stale = self
            .npc_nodes
            .keys()
            .copied()
            .filter(|entity| !active.contains(entity))
            .collect::<Vec<_>>();
        for entity in stale {
            if let Some(mut rendered) = self.npc_nodes.remove(&entity) {
                rendered.node.queue_free();
            }
        }

        let model_meshes = self.model_meshes.clone();
        let model_scenes = self.model_scenes.clone();
        let work_prop_meshes = self.work_prop_meshes.clone();
        let camera_position = self.camera.get_position();
        let mut npc_root = self.npc_root.clone();
        for npc in npcs {
            let recreate = self
                .npc_nodes
                .get(&npc.entity)
                .is_none_or(|rendered| rendered.appearance != npc.appearance);
            if recreate {
                if let Some(mut rendered) = self.npc_nodes.remove(&npc.entity) {
                    rendered.node.queue_free();
                }
                let model = ModelKey3D::Npc(npc.appearance);
                let Some(scene) = model_scenes.get(&model) else {
                    continue;
                };
                let Some(node) = scene.instantiate() else {
                    continue;
                };
                let node = match node.try_cast::<NpcModel3D>() {
                    Ok(node) => node,
                    Err(mut node) => {
                        godot_error!(
                            "WorldRenderer3D: NPC wrapper root is {}, expected NpcModel3D",
                            node.get_class()
                        );
                        node.queue_free();
                        continue;
                    }
                };
                npc_root.add_child(&node);
                self.npc_nodes.insert(
                    npc.entity,
                    RenderedNpc3D {
                        appearance: npc.appearance,
                        node,
                        cargo_kind: None,
                        cargo_node: None,
                        wheelbarrow_kind: None,
                        wheelbarrow: None,
                        wheelbarrow_load: None,
                        work_prop_kind: None,
                        work_prop: None,
                    },
                );
            }

            let Some(rendered) = self.npc_nodes.get_mut(&npc.entity) else {
                continue;
            };
            let position = npc_position_3d(npc.position.coord, npc.position.subtile_offset);
            rendered.node.set_position(position);
            rendered
                .node
                .set_rotation(Vector3::new(0.0, facing_yaw_radians(npc.facing), 0.0));
            rendered
                .node
                .set_visible(position.distance_to(camera_position) <= 66.0);
            rendered.node.bind_mut().set_activity(
                npc.activity,
                npc.carried_kind.is_some(),
                npc.has_wheelbarrow,
            );

            let carried_kind = (!npc.has_wheelbarrow).then_some(npc.carried_kind).flatten();
            if rendered.cargo_kind != carried_kind {
                if let Some(mut cargo) = rendered.cargo_node.take() {
                    cargo.queue_free();
                }
                rendered.cargo_kind = carried_kind;
                if let Some(kind) = carried_kind {
                    if let Some(model) = model_meshes.get(&ModelKey3D::Resource(kind)) {
                        let mut cargo = MeshInstance3D::new_alloc();
                        cargo.set_mesh(&model.mesh);
                        cargo.set_transform(scale_transform(0.28) * model.source_transform);
                        rendered.node.bind().carry_attachment().add_child(&cargo);
                        rendered.cargo_node = Some(cargo);
                    }
                }
            }

            if npc.has_wheelbarrow && rendered.wheelbarrow.is_none() {
                if let Some(scene) = model_scenes.get(&ModelKey3D::Wheelbarrow) {
                    if let Some(node) = scene.instantiate() {
                        match node.try_cast::<WheelbarrowModel3D>() {
                            Ok(mut wheelbarrow) => {
                                wheelbarrow.set_position(Vector3::new(0.0, 0.0, -0.85));
                                rendered
                                    .node
                                    .bind()
                                    .wheelbarrow_attachment()
                                    .add_child(&wheelbarrow);
                                rendered.wheelbarrow = Some(wheelbarrow);
                            }
                            Err(mut node) => node.queue_free(),
                        }
                    }
                }
            } else if !npc.has_wheelbarrow {
                if let Some(mut wheelbarrow) = rendered.wheelbarrow.take() {
                    wheelbarrow.queue_free();
                }
                rendered.wheelbarrow_kind = None;
                rendered.wheelbarrow_load = None;
            }
            if let Some(wheelbarrow) = rendered.wheelbarrow.as_mut() {
                wheelbarrow
                    .bind_mut()
                    .set_rolling(npc.activity == NpcActivity::Walk);
                if rendered.wheelbarrow_kind != npc.wheelbarrow_kind {
                    if let Some(mut load) = rendered.wheelbarrow_load.take() {
                        load.queue_free();
                    }
                    rendered.wheelbarrow_kind = npc.wheelbarrow_kind;
                    if let Some(kind) = npc.wheelbarrow_kind {
                        if let Some(model) = model_meshes.get(&ModelKey3D::Resource(kind)) {
                            let mut load = MeshInstance3D::new_alloc();
                            load.set_mesh(&model.mesh);
                            load.set_transform(scale_transform(0.24) * model.source_transform);
                            wheelbarrow.bind().load_attachment().add_child(&load);
                            rendered.wheelbarrow_load = Some(load);
                        }
                    }
                }
            }

            let work_prop_kind = work_prop_for_activity(npc.activity);
            if rendered.work_prop_kind != work_prop_kind {
                if let Some(mut prop) = rendered.work_prop.take() {
                    prop.queue_free();
                }
                rendered.work_prop_kind = work_prop_kind;
                if let Some(kind) = work_prop_kind {
                    if let Some(parts) = work_prop_meshes.get(&kind) {
                        let prop = instantiate_work_prop(parts);
                        rendered
                            .node
                            .bind()
                            .right_hand_attachment()
                            .add_child(&prop);
                        rendered.work_prop = Some(prop);
                    }
                }
            }
        }
    }

    fn rebuild_pick_proxies(&mut self) {
        let mut proxies: HashMap<ChunkCoord, Vec<PickProxy3D>> = HashMap::new();
        for building in &self.dynamic_snapshot.buildings {
            let origin = footprint_center_3d(building.footprint, 0.0);
            let bounds = self
                .model_meshes
                .get(&ModelKey3D::Building(building.kind))
                .map(|model| model_pick_bounds(model, Transform3D::new(Basis::IDENTITY, origin)))
                .unwrap_or_else(|| {
                    PickAabb::from_center_size(
                        footprint_center_3d(building.footprint, 1.5),
                        Vector3::new(
                            building.footprint.width() as f32 * WORLD_UNITS_PER_TILE,
                            3.0,
                            building.footprint.height() as f32 * WORLD_UNITS_PER_TILE,
                        ),
                    )
                });
            proxies
                .entry(chunk_coord(building.footprint.origin()))
                .or_default()
                .push(PickProxy3D {
                    kind: ProxyKind3D::Building,
                    entity: building.entity,
                    bounds,
                });
        }
        for resource in &self.dynamic_snapshot.resources {
            let origin = cell_center_3d(resource.coord);
            let bounds = self
                .model_meshes
                .get(&ModelKey3D::Resource(resource.kind))
                .map(|model| model_pick_bounds(model, Transform3D::new(Basis::IDENTITY, origin)))
                .unwrap_or_else(|| {
                    PickAabb::from_center_size(
                        origin + Vector3::new(0.0, 0.55, 0.0),
                        Vector3::new(1.4, 1.1, 1.4),
                    )
                });
            proxies
                .entry(chunk_coord(resource.coord))
                .or_default()
                .push(PickProxy3D {
                    kind: ProxyKind3D::Resource,
                    entity: resource.entity,
                    bounds,
                });
        }
        for npc in &self.dynamic_snapshot.npcs {
            let origin = npc_position_3d(npc.position.coord, npc.position.subtile_offset);
            let bounds = self
                .model_meshes
                .get(&ModelKey3D::Npc(npc.appearance))
                .map(|model| model_pick_bounds(model, Transform3D::new(Basis::IDENTITY, origin)))
                .unwrap_or_else(|| {
                    PickAabb::from_center_size(
                        origin + Vector3::new(0.0, 0.9, 0.0),
                        Vector3::new(0.8, 1.8, 0.8),
                    )
                });
            proxies
                .entry(chunk_coord(npc.position.coord))
                .or_default()
                .push(PickProxy3D {
                    kind: ProxyKind3D::Npc,
                    entity: npc.entity,
                    bounds,
                });
        }
        for road in &self.dynamic_snapshot.planned_roads {
            proxies
                .entry(chunk_coord(road.coord))
                .or_default()
                .push(PickProxy3D {
                    kind: ProxyKind3D::RoadBlueprint,
                    entity: road.entity,
                    bounds: PickAabb::from_center_size(
                        cell_center_3d(road.coord) + Vector3::new(0.0, 0.04, 0.0),
                        Vector3::new(1.8, 0.08, 1.8),
                    ),
                });
        }
        for chunk_proxies in proxies.values_mut() {
            chunk_proxies
                .sort_by_key(|proxy| (proxy_kind_priority(proxy.kind), proxy.entity.to_bits()));
        }
        self.pick_proxies = proxies;
    }

    pub(crate) fn proxy_hits(&self, screen_position: Vector2) -> Vec<ProxyHit3D> {
        let ray = self.pointer_ray(screen_position);
        let mut hits = Vec::new();
        for proxies in self.pick_proxies.values() {
            let Some(chunk_bounds) = proxies
                .iter()
                .map(|proxy| proxy.bounds)
                .reduce(PickAabb::merged)
            else {
                continue;
            };
            if ray.intersects_aabb(chunk_bounds).is_none() {
                continue;
            }
            hits.extend(proxies.iter().filter_map(|proxy| {
                ray.intersects_aabb(proxy.bounds)
                    .map(|distance| ProxyHit3D {
                        kind: proxy.kind,
                        entity: proxy.entity,
                        distance,
                    })
            }));
        }
        hits.sort_by(|left, right| {
            proxy_kind_priority(left.kind)
                .cmp(&proxy_kind_priority(right.kind))
                .then_with(|| left.distance.total_cmp(&right.distance))
                .then_with(|| left.entity.to_bits().cmp(&right.entity.to_bits()))
        });
        hits
    }

    fn rebuild_world_overlay(&mut self) {
        for mut node in self.world_overlay_nodes.drain(..) {
            node.queue_free();
        }
        let mut colored = vec![
            (
                Color::from_rgba(1.0, 0.84, 0.0, 0.38),
                OverlayGeometry::default(),
            ),
            (
                Color::from_rgba(0.1, 0.85, 1.0, 0.38),
                OverlayGeometry::default(),
            ),
            (
                Color::from_rgba(1.0, 0.55, 0.12, 0.34),
                OverlayGeometry::default(),
            ),
            (
                Color::from_rgba(0.1, 0.9, 0.45, 0.36),
                OverlayGeometry::default(),
            ),
            (
                Color::from_rgba(1.0, 0.1, 0.1, 0.40),
                OverlayGeometry::default(),
            ),
        ];

        if let Some(selected) = self.overlay_snapshot.selected_cell {
            colored[0].1.append_cell(selected.coord, 0.065, 0.08);
        }
        if let Some(selected) = self.overlay_snapshot.selected_npc {
            colored[1]
                .1
                .append_cell(selected.position.coord, 0.075, 0.08);
        }
        if let Some(selected) = self.overlay_snapshot.selected_building {
            colored[2]
                .1
                .append_footprint(selected.footprint, 0.07, 0.08);
        }
        for cell in &self.overlay_snapshot.plot_cells {
            let group = match cell.validity {
                PlacementValidity::Valid => &mut colored[3].1,
                PlacementValidity::Invalid => &mut colored[4].1,
            };
            group.append_cell(cell.coord, 0.08, 0.10);
        }
        for cell in &self.overlay_snapshot.road_cells {
            let group = match cell.validity {
                PlacementValidity::Valid => &mut colored[3].1,
                PlacementValidity::Invalid => &mut colored[4].1,
            };
            group.append_cell(cell.coord, 0.085, 0.10);
        }
        if let Some(preview) = self.overlay_snapshot.building_preview {
            let (group, material_state) = match preview.validity {
                PlacementValidity::Valid => (&mut colored[3].1, ModelMaterialState3D::PreviewValid),
                PlacementValidity::Invalid => {
                    (&mut colored[4].1, ModelMaterialState3D::PreviewInvalid)
                }
            };
            group.append_footprint(preview.footprint, 0.09, 0.10);
            if let Some(model) = self
                .model_meshes
                .get(&ModelKey3D::Building(preview.kind))
                .cloned()
            {
                let mut node = MeshInstance3D::new_alloc();
                node.set_mesh(&model.mesh);
                node.set_transform(
                    Transform3D::new(
                        Basis::IDENTITY,
                        footprint_center_3d(preview.footprint, 0.02),
                    ) * model.source_transform,
                );
                if let Some(material) = self.override_materials.get(&material_state) {
                    node.set_material_override(material);
                }
                self.overlay_root.add_child(&node);
                self.world_overlay_nodes.push(node);
            }
        }

        if let Some(route) = self.overlay_snapshot.selected_npc_route.clone() {
            match route {
                NpcRouteOverlay::Route {
                    position,
                    waypoints,
                    destination,
                } => {
                    let mut points = Vec::with_capacity(waypoints.len() + 1);
                    points.push(npc_position_3d(position.coord, position.subtile_offset));
                    points.extend(waypoints.into_iter().map(cell_center_3d));
                    for segment in points.windows(2) {
                        colored[1]
                            .1
                            .append_ribbon(segment[0], segment[1], 0.11, 0.075);
                        colored[1].1.append_chevron(segment[0], segment[1], 0.115);
                    }
                    colored[1].1.append_cell(destination, 0.11, 0.15);
                }
                NpcRouteOverlay::Blocked { position } => {
                    let center = npc_position_3d(position.coord, position.subtile_offset);
                    let delta = Vector3::new(0.45, 0.0, 0.45);
                    colored[4]
                        .1
                        .append_ribbon(center - delta, center + delta, 0.12, 0.10);
                    let delta = Vector3::new(-0.45, 0.0, 0.45);
                    colored[4]
                        .1
                        .append_ribbon(center - delta, center + delta, 0.12, 0.10);
                }
            }
        }

        for (color, geometry) in colored {
            let Some(node) = geometry.into_mesh_instance(color) else {
                continue;
            };
            self.overlay_root.add_child(&node);
            self.world_overlay_nodes.push(node);
        }
    }

    pub(crate) fn mark_failed(&mut self, reason: &'static str) {
        self.availability = Renderer3DAvailability::Failed;
        self.failure_reason = Some(reason);
        self.set_active(false);
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        self.base_mut().set_visible(active);
        self.camera.set_current(active);
        if !active {
            self.orbiting = false;
        }
    }

    pub(crate) fn configure_surface(&mut self, width: i32, height: i32, reset_focus: bool) {
        self.camera_state
            .configure_surface(width, height, reset_focus);
        self.sync_camera_transform();
    }

    pub(crate) const fn focus_tiles(&self) -> Vector2 {
        self.camera_state.focus_tiles
    }

    pub(crate) fn set_focus_tiles(&mut self, focus: Vector2) {
        self.camera_state.focus_tiles = focus;
        self.camera_state.clamp();
        self.sync_camera_transform();
    }

    pub(crate) const fn is_orbiting(&self) -> bool {
        self.orbiting
    }

    pub(crate) fn begin_orbit(&mut self) {
        self.orbiting = true;
    }

    pub(crate) fn end_orbit(&mut self) {
        self.orbiting = false;
    }

    pub(crate) fn orbit(&mut self, relative: Vector2) {
        if self.orbiting {
            self.camera_state.orbit(relative);
            self.sync_camera_transform();
        }
    }

    pub(crate) fn pan(&mut self, direction: Vector2, delta: f64) {
        self.camera_state.pan(direction, delta);
        self.sync_camera_transform();
    }

    pub(crate) fn dolly(&mut self, factor: f32) {
        self.camera_state.dolly(factor);
        self.sync_camera_transform();
    }

    pub(crate) fn pointer_ray(&self, screen_position: Vector2) -> Ray3 {
        Ray3 {
            origin: self.camera.project_ray_origin(screen_position),
            direction: self.camera.project_ray_normal(screen_position).normalized(),
        }
    }

    fn sync_camera_transform(&mut self) {
        let position = self.camera_state.camera_world();
        let target = self.camera_state.focus_world();
        self.camera.set_position(position);
        self.camera.look_at(target);
        self.sync_npc_visibility();
    }

    fn sync_npc_visibility(&mut self) {
        let camera_position = self.camera.get_position();
        for rendered in self.npc_nodes.values_mut() {
            let position = rendered.node.get_position();
            rendered
                .node
                .set_visible(position.distance_to(camera_position) <= 66.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ray3 {
    pub(crate) origin: Vector3,
    pub(crate) direction: Vector3,
}

impl Ray3 {
    pub(crate) fn ground_intersection(self) -> Option<Vector3> {
        if !self.origin.is_finite() || !self.direction.is_finite() {
            return None;
        }
        if self.direction.y.abs() <= f32::EPSILON {
            return None;
        }
        let distance = -self.origin.y / self.direction.y;
        (distance >= 0.0).then_some(self.origin + self.direction * distance)
    }

    pub(crate) fn intersects_aabb(self, bounds: PickAabb) -> Option<f32> {
        let mut near = 0.0_f32;
        let mut far = f32::INFINITY;
        for (origin, direction, min, max) in [
            (self.origin.x, self.direction.x, bounds.min.x, bounds.max.x),
            (self.origin.y, self.direction.y, bounds.min.y, bounds.max.y),
            (self.origin.z, self.direction.z, bounds.min.z, bounds.max.z),
        ] {
            if direction.abs() <= f32::EPSILON {
                if origin < min || origin > max {
                    return None;
                }
                continue;
            }
            let inverse = direction.recip();
            let mut first = (min - origin) * inverse;
            let mut second = (max - origin) * inverse;
            if first > second {
                std::mem::swap(&mut first, &mut second);
            }
            near = near.max(first);
            far = far.min(second);
            if near > far {
                return None;
            }
        }
        (far >= 0.0).then_some(near.max(0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PickAabb {
    pub(crate) min: Vector3,
    pub(crate) max: Vector3,
}

impl PickAabb {
    pub(crate) fn from_center_size(center: Vector3, size: Vector3) -> Self {
        let half = size * 0.5;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    fn merged(self, other: Self) -> Self {
        Self {
            min: Vector3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Vector3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ChunkCoord {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

pub(crate) fn cell_center_3d(coord: CellCoord) -> Vector3 {
    Vector3::new(
        (coord.x() as f32 + 0.5) * WORLD_UNITS_PER_TILE,
        0.0,
        (coord.y() as f32 + 0.5) * WORLD_UNITS_PER_TILE,
    )
}

pub(crate) fn npc_position_3d(coord: CellCoord, offset: SubtileOffset) -> Vector3 {
    let center = cell_center_3d(coord);
    Vector3::new(
        center.x + offset.x_units as f32 / 1024.0 * WORLD_UNITS_PER_TILE,
        0.0,
        center.z + offset.y_units as f32 / 1024.0 * WORLD_UNITS_PER_TILE,
    )
}

#[cfg(test)]
pub(crate) fn ground_point_to_cell(point: Vector3, width: i32, height: i32) -> Option<CellCoord> {
    if !point.is_finite() || point.x < 0.0 || point.z < 0.0 {
        return None;
    }
    let x = (point.x / WORLD_UNITS_PER_TILE).floor() as i32;
    let y = (point.z / WORLD_UNITS_PER_TILE).floor() as i32;
    (x >= 0 && y >= 0 && x < width && y < height).then(|| CellCoord::new(x, y))
}

pub(crate) fn chunk_coord(coord: CellCoord) -> ChunkCoord {
    ChunkCoord {
        x: coord.x().div_euclid(CHUNK_SIZE_TILES),
        y: coord.y().div_euclid(CHUNK_SIZE_TILES),
    }
}

#[cfg(test)]
pub(crate) fn dirty_road_chunks(
    previous: &HashSet<CellCoord>,
    current: &HashSet<CellCoord>,
) -> BTreeSet<ChunkCoord> {
    let changed = previous
        .symmetric_difference(current)
        .copied()
        .collect::<Vec<_>>();
    let mut chunks = BTreeSet::new();
    for coord in changed {
        for (dx, dy) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
            let Some(x) = coord.x().checked_add(dx) else {
                continue;
            };
            let Some(y) = coord.y().checked_add(dy) else {
                continue;
            };
            if x >= 0 && y >= 0 {
                chunks.insert(chunk_coord(CellCoord::new(x, y)));
            }
        }
    }
    chunks
}

fn static_visuals_changed(
    previous: &DynamicRenderSnapshot,
    current: &DynamicRenderSnapshot,
) -> bool {
    previous.buildings != current.buildings
        || previous.crops != current.crops
        || previous.tree_plots != current.tree_plots
        || previous.resources.len() != current.resources.len()
        || previous
            .resources
            .iter()
            .zip(&current.resources)
            .any(|(left, right)| {
                left.entity != right.entity || left.coord != right.coord || left.kind != right.kind
            })
}

fn max_camera_distance_world(width: i32, height: i32) -> f32 {
    let diagonal_tiles = (width as f32).hypot(height as f32).max(MIN_DISTANCE_TILES);
    diagonal_tiles * MAX_DISTANCE_DIAGONAL_FACTOR * WORLD_UNITS_PER_TILE
}

fn textured_material(
    texture: Gd<godot::classes::Texture2D>,
    tint: Color,
    transparent: bool,
) -> Gd<Material> {
    let mut material = StandardMaterial3D::new_gd();
    material.set_texture(base_material_3d::TextureParam::ALBEDO, &texture);
    material.set_albedo(tint);
    material.set_roughness(0.82);
    material.set_texture_filter(base_material_3d::TextureFilter::LINEAR_WITH_MIPMAPS);
    if transparent {
        material.set_transparency(base_material_3d::Transparency::ALPHA);
    }
    material.upcast()
}

fn build_override_materials() -> HashMap<ModelMaterialState3D, Gd<Material>> {
    [
        (
            ModelMaterialState3D::Blueprint,
            Color::from_rgba(0.15, 0.85, 1.0, 0.62),
        ),
        (
            ModelMaterialState3D::PreviewValid,
            Color::from_rgba(0.1, 0.9, 0.45, 0.58),
        ),
        (
            ModelMaterialState3D::PreviewInvalid,
            Color::from_rgba(1.0, 0.1, 0.1, 0.58),
        ),
    ]
    .into_iter()
    .map(|(state, color)| {
        let mut material = StandardMaterial3D::new_gd();
        material.set_albedo(color);
        material.set_transparency(base_material_3d::Transparency::ALPHA);
        material.set_shading_mode(base_material_3d::ShadingMode::UNSHADED);
        (state, material.upcast())
    })
    .collect()
}

fn build_model_batches(
    groups: BTreeMap<ModelGroupKey3D, Vec<Transform3D>>,
    model_meshes: &HashMap<ModelKey3D, ModelMesh3D>,
    overview_mesh: Option<&Gd<Mesh>>,
    override_materials: &HashMap<ModelMaterialState3D, Gd<Material>>,
    root: &mut Gd<Node3D>,
) -> BTreeMap<ModelGroupKey3D, Gd<MultiMeshInstance3D>> {
    let mut batches = BTreeMap::new();
    for (key, transforms) in groups {
        let Some((mesh, source_transform)) = (match key.lod {
            ModelLod3D::Detailed => model_meshes
                .get(&key.model)
                .map(|model| (model.mesh.clone(), model.source_transform)),
            ModelLod3D::Overview => overview_mesh
                .cloned()
                .map(|mesh| (mesh, Transform3D::IDENTITY)),
        }) else {
            continue;
        };
        let Ok(instance_count) = i32::try_from(transforms.len()) else {
            continue;
        };
        let mut multimesh = MultiMesh::new_gd();
        multimesh.set_transform_format(multi_mesh::TransformFormat::TRANSFORM_3D);
        multimesh.set_mesh(&mesh);
        multimesh.set_instance_count(instance_count);
        for (index, transform) in transforms.into_iter().enumerate() {
            multimesh.set_instance_transform(index as i32, transform * source_transform);
        }
        let mut node = MultiMeshInstance3D::new_alloc();
        node.set_multimesh(&multimesh);
        if key.material_state != ModelMaterialState3D::Constructed {
            if let Some(material) = override_materials.get(&key.material_state) {
                node.set_material_override(material);
            }
        }
        let cutoff = detailed_cutoff_world(key.model);
        match key.lod {
            ModelLod3D::Detailed => {
                node.set_visibility_range_end(cutoff);
                node.set_visibility_range_end_margin(6.0);
            }
            ModelLod3D::Overview => {
                node.set_visibility_range_begin((cutoff - 4.0).max(0.0));
                node.set_visibility_range_begin_margin(6.0);
            }
        }
        root.add_child(&node);
        batches.insert(key, node);
    }
    batches
}

fn push_model_instance(
    groups: &mut BTreeMap<ModelGroupKey3D, Vec<Transform3D>>,
    coord: CellCoord,
    model: ModelKey3D,
    material_state: ModelMaterialState3D,
    detailed: Transform3D,
    overview: Transform3D,
) {
    let chunk = chunk_coord(coord);
    groups
        .entry(ModelGroupKey3D::new(
            chunk,
            model,
            material_state,
            ModelLod3D::Detailed,
        ))
        .or_default()
        .push(detailed);
    groups
        .entry(ModelGroupKey3D::new(
            chunk,
            model,
            material_state,
            ModelLod3D::Overview,
        ))
        .or_default()
        .push(overview);
}

fn footprint_center_3d(footprint: BuildingFootprint, y: f32) -> Vector3 {
    let origin = footprint.origin();
    Vector3::new(
        (origin.x() as f32 + footprint.width() as f32 * 0.5) * WORLD_UNITS_PER_TILE,
        y,
        (origin.y() as f32 + footprint.height() as f32 * 0.5) * WORLD_UNITS_PER_TILE,
    )
}

fn overview_transform_for_footprint(footprint: BuildingFootprint) -> Transform3D {
    let size = Vector3::new(
        footprint.width() as f32 * WORLD_UNITS_PER_TILE * 0.88,
        1.2 + (footprint.width().max(footprint.height()) as f32 * 0.3),
        footprint.height() as f32 * WORLD_UNITS_PER_TILE * 0.88,
    );
    Transform3D::new(
        Basis::IDENTITY.scaled(size),
        footprint_center_3d(footprint, size.y * 0.5),
    )
}

fn overview_transform(coord: CellCoord, size: Vector3) -> Transform3D {
    Transform3D::new(
        Basis::IDENTITY.scaled(size),
        cell_center_3d(coord) + Vector3::new(0.0, size.y * 0.5, 0.0),
    )
}

fn detailed_cutoff_world(model: ModelKey3D) -> f32 {
    match model {
        ModelKey3D::Building(_) | ModelKey3D::Tree(_) => 64.0 * WORLD_UNITS_PER_TILE,
        ModelKey3D::Resource(_) | ModelKey3D::Crop(_) => 48.0 * WORLD_UNITS_PER_TILE,
        ModelKey3D::Npc(_) | ModelKey3D::Wheelbarrow | ModelKey3D::WorkProps => {
            32.0 * WORLD_UNITS_PER_TILE
        }
    }
}

fn facing_yaw_radians(facing: MovementFacing) -> f32 {
    let degrees: f32 = match facing {
        MovementFacing::North => 0.0,
        MovementFacing::NorthEast => -45.0,
        MovementFacing::East => -90.0,
        MovementFacing::SouthEast => -135.0,
        MovementFacing::South => 180.0,
        MovementFacing::SouthWest => 135.0,
        MovementFacing::West => 90.0,
        MovementFacing::NorthWest => 45.0,
    };
    degrees.to_radians()
}

fn scale_transform(scale: f32) -> Transform3D {
    Transform3D::new(Basis::IDENTITY.scaled(Vector3::splat(scale)), Vector3::ZERO)
}

const fn work_prop_for_activity(activity: NpcActivity) -> Option<WorkPropModel3D> {
    match activity {
        NpcActivity::Gather => Some(WorkPropModel3D::Gather),
        NpcActivity::Saw => Some(WorkPropModel3D::Saw),
        NpcActivity::Stonecut => Some(WorkPropModel3D::Stonecut),
        NpcActivity::Cook => Some(WorkPropModel3D::Cook),
        NpcActivity::Idle | NpcActivity::Walk => None,
    }
}

fn instantiate_work_prop(parts: &[ModelMesh3D]) -> Gd<Node3D> {
    let bounds = parts
        .iter()
        .map(|part| part.source_transform * part.mesh.get_aabb())
        .reduce(Aabb::merge);
    let center = bounds.map_or(Vector3::ZERO, Aabb::center);
    let center_correction = Transform3D::new(Basis::IDENTITY, -center);
    let mut root = Node3D::new_alloc();
    root.set_scale(Vector3::splat(0.72));
    for part in parts {
        let mut node = MeshInstance3D::new_alloc();
        node.set_mesh(&part.mesh);
        node.set_transform(center_correction * part.source_transform);
        root.add_child(&node);
    }
    root
}

fn model_pick_bounds(model: &ModelMesh3D, placement: Transform3D) -> PickAabb {
    let bounds = (placement * model.source_transform) * model.mesh.get_aabb();
    let first = bounds.position;
    let second = bounds.end();
    PickAabb {
        min: Vector3::new(
            first.x.min(second.x),
            first.y.min(second.y),
            first.z.min(second.z),
        ),
        max: Vector3::new(
            first.x.max(second.x),
            first.y.max(second.y),
            first.z.max(second.z),
        ),
    }
}

const fn proxy_kind_priority(kind: ProxyKind3D) -> u8 {
    match kind {
        ProxyKind3D::Building => 0,
        ProxyKind3D::RoadBlueprint => 1,
        ProxyKind3D::Npc => 2,
        ProxyKind3D::Resource => 3,
    }
}

#[derive(Default)]
struct OverlayGeometry {
    vertices: Vec<Vector3>,
    indices: Vec<i32>,
}

impl OverlayGeometry {
    fn append_quad(&mut self, points: [Vector3; 4]) {
        let Ok(first) = i32::try_from(self.vertices.len()) else {
            return;
        };
        self.vertices.extend(points);
        self.indices
            .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }

    fn append_cell(&mut self, coord: CellCoord, y: f32, inset: f32) {
        let min_x = coord.x() as f32 * WORLD_UNITS_PER_TILE + inset;
        let min_z = coord.y() as f32 * WORLD_UNITS_PER_TILE + inset;
        let max_x = (coord.x() as f32 + 1.0) * WORLD_UNITS_PER_TILE - inset;
        let max_z = (coord.y() as f32 + 1.0) * WORLD_UNITS_PER_TILE - inset;
        self.append_quad([
            Vector3::new(min_x, y, min_z),
            Vector3::new(max_x, y, min_z),
            Vector3::new(max_x, y, max_z),
            Vector3::new(min_x, y, max_z),
        ]);
    }

    fn append_footprint(&mut self, footprint: BuildingFootprint, y: f32, inset: f32) {
        let origin = footprint.origin();
        let min_x = origin.x() as f32 * WORLD_UNITS_PER_TILE + inset;
        let min_z = origin.y() as f32 * WORLD_UNITS_PER_TILE + inset;
        let max_x = (origin.x() as f32 + footprint.width() as f32) * WORLD_UNITS_PER_TILE - inset;
        let max_z = (origin.y() as f32 + footprint.height() as f32) * WORLD_UNITS_PER_TILE - inset;
        self.append_quad([
            Vector3::new(min_x, y, min_z),
            Vector3::new(max_x, y, min_z),
            Vector3::new(max_x, y, max_z),
            Vector3::new(min_x, y, max_z),
        ]);
    }

    fn append_ribbon(&mut self, from: Vector3, to: Vector3, y: f32, half_width: f32) {
        let direction = Vector2::new(to.x - from.x, to.z - from.z);
        if direction.length_squared() <= f32::EPSILON {
            return;
        }
        let perpendicular = Vector2::new(-direction.y, direction.x).normalized() * half_width;
        self.append_quad([
            Vector3::new(from.x + perpendicular.x, y, from.z + perpendicular.y),
            Vector3::new(to.x + perpendicular.x, y, to.z + perpendicular.y),
            Vector3::new(to.x - perpendicular.x, y, to.z - perpendicular.y),
            Vector3::new(from.x - perpendicular.x, y, from.z - perpendicular.y),
        ]);
    }

    fn append_chevron(&mut self, from: Vector3, to: Vector3, y: f32) {
        let delta = Vector2::new(to.x - from.x, to.z - from.z);
        if delta.length_squared() <= f32::EPSILON {
            return;
        }
        let direction = delta.normalized();
        let perpendicular = Vector2::new(-direction.y, direction.x);
        let center = Vector2::new(from.x, from.z).lerp(Vector2::new(to.x, to.z), 0.5);
        let tip = center + direction * 0.24;
        for side in [-1.0, 1.0] {
            let back = center - direction * 0.18 + perpendicular * 0.18 * side;
            self.append_ribbon(
                Vector3::new(back.x, y, back.y),
                Vector3::new(tip.x, y, tip.y),
                y,
                0.025,
            );
        }
    }

    fn into_mesh_instance(self, color: Color) -> Option<Gd<MeshInstance3D>> {
        if self.vertices.is_empty() {
            return None;
        }
        let mut arrays = VarArray::new();
        arrays.resize(mesh::ArrayType::MAX.ord() as usize, &Variant::nil());
        arrays.set(
            mesh::ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(self.vertices.as_slice()).to_variant(),
        );
        arrays.set(
            mesh::ArrayType::INDEX.ord() as usize,
            &PackedInt32Array::from(self.indices.as_slice()).to_variant(),
        );
        let mut mesh = ArrayMesh::new_gd();
        mesh.add_surface_from_arrays(mesh::PrimitiveType::TRIANGLES, &arrays);
        let mut node = MeshInstance3D::new_alloc();
        let mesh: Gd<Mesh> = mesh.upcast();
        node.set_mesh(&mesh);
        let material = overlay_material(color);
        node.set_material_override(&material);
        Some(node)
    }
}

fn overlay_material(color: Color) -> Gd<Material> {
    let mut material = StandardMaterial3D::new_gd();
    material.set_albedo(color);
    material.set_transparency(base_material_3d::Transparency::ALPHA);
    material.set_shading_mode(base_material_3d::ShadingMode::UNSHADED);
    material.set_cull_mode(base_material_3d::CullMode::DISABLED);
    material.upcast()
}

fn build_grid_overlay(snapshot: &SurfaceRenderSnapshot) -> Option<Gd<MeshInstance3D>> {
    let width = snapshot.size.width_i32()?;
    let height = snapshot.size.height_i32()?;
    let world_width = width as f32 * WORLD_UNITS_PER_TILE;
    let world_height = height as f32 * WORLD_UNITS_PER_TILE;
    let mut geometry = OverlayGeometry::default();
    for x in 0..=width {
        let x = x as f32 * WORLD_UNITS_PER_TILE;
        geometry.append_ribbon(
            Vector3::new(x, 0.028, 0.0),
            Vector3::new(x, 0.028, world_height),
            0.028,
            0.012,
        );
    }
    for y in 0..=height {
        let z = y as f32 * WORLD_UNITS_PER_TILE;
        geometry.append_ribbon(
            Vector3::new(0.0, 0.028, z),
            Vector3::new(world_width, 0.028, z),
            0.028,
            0.012,
        );
    }
    geometry.into_mesh_instance(Color::from_rgba(0.15, 0.24, 0.20, 0.28))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::render_snapshot::ResourceRenderInfo;
    use bevy_ecs::world::World;

    #[test]
    fn cell_and_subtile_coordinates_map_to_xz_world_space() {
        assert_eq!(
            cell_center_3d(CellCoord::new(2, 3)),
            Vector3::new(5.0, 0.0, 7.0)
        );
        assert_eq!(
            npc_position_3d(CellCoord::new(2, 3), SubtileOffset::new(512, -512)),
            Vector3::new(6.0, 0.0, 6.0)
        );
        assert_eq!(
            ground_point_to_cell(Vector3::new(5.9, 0.0, 7.9), 10, 10),
            Some(CellCoord::new(2, 3))
        );
    }

    #[test]
    fn chunk_boundaries_use_thirty_two_tiles() {
        assert_eq!(
            chunk_coord(CellCoord::new(31, 31)),
            ChunkCoord { x: 0, y: 0 }
        );
        assert_eq!(
            chunk_coord(CellCoord::new(32, 32)),
            ChunkCoord { x: 1, y: 1 }
        );
    }

    #[test]
    fn camera_clamps_pitch_distance_and_focus() {
        let mut state = OrbitCameraState::default();
        state.configure_surface(10, 20, true);
        state.elevation_radians = 0.0;
        state.distance_world = 1.0;
        state.focus_tiles = Vector2::new(-4.0, 80.0);
        state.clamp();
        assert_eq!(state.elevation_radians, MIN_ELEVATION_DEGREES.to_radians());
        assert_eq!(
            state.distance_world,
            MIN_DISTANCE_TILES * WORLD_UNITS_PER_TILE
        );
        assert_eq!(state.focus_tiles, Vector2::new(0.0, 20.0));

        state.distance_world = f32::MAX;
        state.clamp();
        assert_eq!(state.distance_world, max_camera_distance_world(10, 20));
    }

    #[test]
    fn ray_intersects_ground_and_aabb_in_front_only() {
        let ray = Ray3 {
            origin: Vector3::new(0.0, 10.0, 0.0),
            direction: Vector3::new(0.0, -1.0, 0.0),
        };
        assert_eq!(ray.ground_intersection(), Some(Vector3::ZERO));
        let bounds =
            PickAabb::from_center_size(Vector3::new(0.0, 4.0, 0.0), Vector3::new(2.0, 2.0, 2.0));
        assert_eq!(ray.intersects_aabb(bounds), Some(5.0));

        let away = Ray3 {
            origin: Vector3::new(0.0, 10.0, 0.0),
            direction: Vector3::UP,
        };
        assert_eq!(away.ground_intersection(), None);
        assert_eq!(away.intersects_aabb(bounds), None);
    }

    #[test]
    fn road_diffs_dirty_changed_and_neighbor_chunks() {
        let previous = HashSet::from([CellCoord::new(31, 5)]);
        let current = HashSet::from([CellCoord::new(32, 5)]);
        let dirty = dirty_road_chunks(&previous, &current);
        assert_eq!(
            dirty,
            BTreeSet::from([ChunkCoord { x: 0, y: 0 }, ChunkCoord { x: 1, y: 0 }])
        );
    }

    #[test]
    fn static_visual_diff_ignores_resource_quantity_only_changes() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let resource = |quantity| ResourceRenderInfo {
            entity,
            coord: CellCoord::new(3, 4),
            kind: ResourceKind::Wood,
            quantity,
        };
        let previous = DynamicRenderSnapshot {
            resources: vec![resource(10)],
            ..Default::default()
        };
        let quantity_changed = DynamicRenderSnapshot {
            resources: vec![resource(9)],
            ..Default::default()
        };
        assert!(!static_visuals_changed(&previous, &quantity_changed));

        let mut moved = quantity_changed;
        moved.resources[0].coord = CellCoord::new(4, 4);
        assert!(static_visuals_changed(&previous, &moved));
    }

    #[test]
    fn work_activities_select_distinct_typed_prop_groups() {
        assert_eq!(
            work_prop_for_activity(NpcActivity::Gather),
            Some(WorkPropModel3D::Gather)
        );
        assert_eq!(
            work_prop_for_activity(NpcActivity::Saw),
            Some(WorkPropModel3D::Saw)
        );
        assert_eq!(
            work_prop_for_activity(NpcActivity::Stonecut),
            Some(WorkPropModel3D::Stonecut)
        );
        assert_eq!(
            work_prop_for_activity(NpcActivity::Cook),
            Some(WorkPropModel3D::Cook)
        );
        assert_eq!(work_prop_for_activity(NpcActivity::Idle), None);
        assert_eq!(work_prop_for_activity(NpcActivity::Walk), None);
    }

    #[test]
    fn pick_bounds_merge_into_a_chunk_broad_phase() {
        let first = PickAabb::from_center_size(Vector3::ZERO, Vector3::splat(2.0));
        let second =
            PickAabb::from_center_size(Vector3::new(4.0, 2.0, -3.0), Vector3::new(2.0, 4.0, 2.0));
        assert_eq!(
            first.merged(second),
            PickAabb {
                min: Vector3::new(-1.0, -1.0, -4.0),
                max: Vector3::new(5.0, 4.0, 1.0),
            }
        );
    }
}
