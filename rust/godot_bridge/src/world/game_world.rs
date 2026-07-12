use crate::assets::{
    building_asset_path, load_packed_scene, load_texture, npc_scene_path, resource_asset_path,
    road_asset_path, terrain_asset_path,
};
use crate::entity_id::BridgeEntityId;
use crate::panel::construction_dock::ConstructionDock;
use crate::world::render_snapshot as snapshot;
use crate::world::visual::{WorldArtMetrics, LOGICAL_TILE_SIZE};
use crate::world::world_renderer_2d::WorldRenderer2D;
use crate::world::world_renderer_3d::{
    ProxyHit3D, ProxyKind3D, Renderer3DAvailability, WorldRenderer3D, WORLD_UNITS_PER_TILE,
};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, BuildingKind, BuildingPlacementError,
};
use game_engine::components::{
    MovementFacing, NpcAppearance, SubtileOffset, TerrainKind, Tile, TilePosition, Velocity,
    SUBTILE_UNITS_PER_TILE,
};
use game_engine::farming::FieldCropState;
use game_engine::forestry::TreePlotState;
use game_engine::grid::{self, CellCoord, Grid, WorldPosition};
use game_engine::navigation::NpcRoute;
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::{ResourceAmounts, ResourceKind, ResourceOverview};
use game_engine::roads::{
    RoadBlueprint, RoadMap, RoadPlacementBatchResult, RoadPlacementError, RoadTier,
};
use game_engine::simulation::{
    BuildingCommandError, BuildingTarget, GameSimulation, SimulationSpeed, SurfaceId,
};
use game_engine::tile::TileIndex;
use godot::classes::{
    canvas_item::TextureFilter, input, AnimatedSprite2D, AtlasTexture, Camera2D, INode, Input,
    InputEvent, InputEventMouseButton, InputEventMouseMotion, Node, PackedScene, Polygon2D,
    Sprite2D, SpriteFrames, Texture2D, TileMapLayer, TileSet, TileSetAtlasSource, TileSetSource,
};
use godot::global::MouseButton;
use godot::obj::{OnEditor, Singleton};
use godot::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use game_engine::components::{AiGatherResource, CarriedResource, Wheelbarrow};
#[cfg(test)]
use game_engine::farming::{field_crop_state, AiHarvestField, AiSeedField, FieldCrop};
#[cfg(test)]
use game_engine::forestry::{tree_plot_state, AiCutTreePlot, AiSeedTreePlot, TreePlotGrowth};
#[cfg(test)]
use game_engine::refining::{AiRefineResource, RecipeKind};
#[cfg(test)]
use game_engine::roads::Road;

const ZOOM_ABSOLUTE_FLOOR: f32 = 0.001;
const ZOOM_MARGIN: f32 = 0.95;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_FACTOR: f32 = 1.1;
const PAN_SPEED: f32 = 600.0;
const ACTION_CAMERA_PAN_UP: &str = "camera_pan_up";
const ACTION_CAMERA_PAN_DOWN: &str = "camera_pan_down";
const ACTION_CAMERA_PAN_LEFT: &str = "camera_pan_left";
const ACTION_CAMERA_PAN_RIGHT: &str = "camera_pan_right";
const ACTION_MENU_TOGGLE: &str = "menu_toggle";
const WORLD_ENTITY_Z_BASE: i32 = 5;
const TERRAIN_VARIANT_COUNT: i32 = 4;
const WHEELBARROW_OVERLAY_SCENE_PATH: &str = "res://world/wheelbarrow_overlay.tscn";
const WHEELBARROW_EMPTY_PATH: &str =
    "res://assets/visual/world/vehicles/wheelbarrow_empty_sheet.png";
const CROP_SEEDABLE_PATH: &str = "res://assets/visual/world/farming/crop_seedable_plot.png";
const CROP_GROWING_STEP1_PATH: &str = "res://assets/visual/world/farming/crop_growing_step1.png";
const CROP_GROWING_STEP2_PATH: &str = "res://assets/visual/world/farming/crop_growing_step2.png";
const CROP_GROWN_PATH: &str = "res://assets/visual/world/farming/crop_grown.png";
const TREE_PLOT_SAPLING_PATH: &str = "res://assets/visual/world/farming/tree_plot_sapling.png";
const TREE_PLOT_YOUNG_PATH: &str = "res://assets/visual/world/farming/tree_plot_young.png";
const TREE_PLOT_MATURE_PATH: &str = "res://assets/visual/world/farming/tree_plot_mature.png";
static GENERATION_SEED_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fresh_generation_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded_time = nanos as u64 ^ (nanos >> 64) as u64;
    folded_time ^ GENERATION_SEED_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedCell {
    coord: CellCoord,
    entity: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedNpc {
    coord: CellCoord,
    entity: Entity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectedNpcRouteOverlay {
    Route {
        position: NpcPosition,
        waypoints: Vec<CellCoord>,
        destination: CellCoord,
    },
    Blocked {
        position: NpcPosition,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedBuilding {
    footprint: BuildingFootprint,
    entity: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ClickSelectionTargets {
    tile: Option<SelectedCell>,
    npc: Option<SelectedNpc>,
    building: Option<SelectedBuilding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionEvent {
    TileSelected(Entity),
    TileDeselected,
    NpcSelected(Entity),
    NpcDeselected,
    BuildingSelected(Entity),
    BuildingDeselected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, GodotConvert, Var, Export)]
#[godot(via = i64)]
pub(crate) enum RendererMode {
    TwoD = 0,
    ThreeD = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapEntityKind {
    Building,
    Npc,
    ResourceNode,
    RoadBlueprint,
}

impl MapEntityKind {
    pub(crate) const fn signal_value(self) -> i64 {
        match self {
            Self::Building => 0,
            Self::Npc => 1,
            Self::ResourceNode => 2,
            Self::RoadBlueprint => 3,
        }
    }

    pub(crate) const fn from_signal_value(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Building),
            1 => Some(Self::Npc),
            2 => Some(Self::ResourceNode),
            3 => Some(Self::RoadBlueprint),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MapEntityTarget {
    kind: MapEntityKind,
    entity: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NpcRefiningAnimation {
    Saw,
    Stonecut,
    Cook,
}

impl NpcRefiningAnimation {
    #[cfg(test)]
    const fn from_recipe(recipe: RecipeKind) -> Self {
        match recipe {
            RecipeKind::SawWood => Self::Saw,
            RecipeKind::CutStone => Self::Stonecut,
            RecipeKind::CookCrops | RecipeKind::CookWildBerries => Self::Cook,
        }
    }

    const fn animation_name(self) -> &'static str {
        match self {
            Self::Saw => "saw",
            Self::Stonecut => "stonecut",
            Self::Cook => "cook",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NpcRenderInfo {
    entity: Entity,
    appearance: NpcAppearance,
    coord: CellCoord,
    subtile_offset: SubtileOffset,
    velocity: Velocity,
    facing: MovementFacing,
    is_gathering: bool,
    refining_animation: Option<NpcRefiningAnimation>,
    carried_kind: Option<ResourceKind>,
    has_wheelbarrow: bool,
    wheelbarrow_kind: Option<ResourceKind>,
}

struct RenderedNpcSprite {
    appearance: NpcAppearance,
    sprite: Gd<AnimatedSprite2D>,
    shadow: Gd<Polygon2D>,
    cargo_icon: Gd<Sprite2D>,
    carried_kind: Option<ResourceKind>,
    wheelbarrow: Gd<AnimatedSprite2D>,
    has_wheelbarrow: bool,
}

struct BuiltTileSet {
    tile_set: Gd<TileSet>,
    metrics: WorldArtMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickPerformanceSample {
    pub(crate) sequence: u64,
    pub(crate) wall_time: Duration,
    pub(crate) fixed_tick_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildingRenderInfo {
    entity: Entity,
    kind: BuildingKind,
    footprint: BuildingFootprint,
    state: BuildingRenderState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CropRenderInfo {
    coord: CellCoord,
    state: FieldCropState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TreePlotRenderInfo {
    coord: CellCoord,
    state: TreePlotState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildingRenderState {
    Blueprint,
    Constructed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlotOwner {
    Farm(Entity),
    ForesterLodge(Entity),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlacementMode {
    Building(BuildingKind),
    Plots {
        owner: PlotOwner,
        drag_cells: Vec<CellCoord>,
    },
    Roads {
        tier: RoadTier,
        drag_cells: Vec<CellCoord>,
        last_rejection: Option<RoadPlacementBatchResult>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlotPlacementPreview {
    coord: CellCoord,
    valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoadPlacementStatus {
    pub(crate) active_tier: Option<RoadTier>,
    pub(crate) cell_count: usize,
    pub(crate) invalid_cell_count: usize,
    pub(crate) aggregate_cost: ResourceAmounts,
    pub(crate) errors: Vec<(CellCoord, RoadPlacementError)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ConstructionTool {
    Building(BuildingKind),
    Road(RoadTier),
    Field,
    TreePlot,
}

impl ConstructionTool {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Building(BuildingKind::TownHall) => "Town Hall",
            Self::Building(kind) => kind.label(),
            Self::Road(tier) => tier.label(),
            Self::Field => "Field",
            Self::TreePlot => "Tree Plot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuildingPlacementFeedback {
    MoveCursorOverMap,
    Valid,
    Invalid(BuildingPlacementError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConstructionPlacementStatus {
    pub(crate) active_tool: Option<ConstructionTool>,
    pub(crate) building_feedback: Option<BuildingPlacementFeedback>,
    pub(crate) road: Option<RoadPlacementStatus>,
}

#[derive(GodotClass)]
#[class(base = Node)]
pub(crate) struct GameWorld {
    #[export]
    renderer_2d: OnEditor<Gd<WorldRenderer2D>>,

    #[export]
    renderer_3d: OnEditor<Gd<WorldRenderer3D>>,

    game: GameSimulation,
    rendered_surface: SurfaceId,
    selected_cell: Option<SelectedCell>,
    selected_npc: Option<SelectedNpc>,
    selected_npc_route_overlay: Option<SelectedNpcRouteOverlay>,
    selected_building: Option<SelectedBuilding>,
    hovered_map_entity: Option<MapEntityTarget>,
    placement_mode: Option<PlacementMode>,
    _tile_set: Option<Gd<TileSet>>,
    _resource_node_tile_set: Option<Gd<TileSet>>,
    _crop_tile_set: Option<Gd<TileSet>>,
    _tree_plot_tile_set: Option<Gd<TileSet>>,
    _road_tile_set: Option<Gd<TileSet>>,
    npc_scenes: HashMap<NpcAppearance, Gd<PackedScene>>,
    wheelbarrow_scene: Option<Gd<PackedScene>>,
    wheelbarrow_frames: Option<Gd<SpriteFrames>>,
    wheelbarrow_overlay_scale: Vector2,
    npc_sprites: HashMap<Entity, RenderedNpcSprite>,
    building_textures: HashMap<BuildingKind, Gd<Texture2D>>,
    building_sprites: HashMap<Entity, Gd<Sprite2D>>,
    building_shadows: HashMap<Entity, Gd<Polygon2D>>,
    tick_performance_sample: Option<TickPerformanceSample>,
    tick_performance_sequence: u64,
    active_renderer_mode: RendererMode,
    surface_snapshot: Option<snapshot::SurfaceRenderSnapshot>,
    dynamic_snapshot: snapshot::DynamicRenderSnapshot,
    overlay_snapshot: snapshot::WorldOverlaySnapshot,
    snapshot_revision: u64,
    renderer_2d_revision: u64,
    renderer_3d_revision: u64,
    surface_generation: u64,
    renderer_2d_surface_generation: u64,
    orbit_previous_mouse_mode: Option<input::MouseMode>,

    base: Base<Node>,
}

#[godot_api]
impl INode for GameWorld {
    fn init(base: Base<Node>) -> Self {
        let game = GameSimulation::new(fresh_generation_seed());
        let rendered_surface = game.default_surface_id();

        Self {
            renderer_2d: OnEditor::default(),
            renderer_3d: OnEditor::default(),
            game,
            rendered_surface,
            selected_cell: None,
            selected_npc: None,
            selected_npc_route_overlay: None,
            selected_building: None,
            hovered_map_entity: None,
            placement_mode: None,
            _tile_set: None,
            _resource_node_tile_set: None,
            _crop_tile_set: None,
            _tree_plot_tile_set: None,
            _road_tile_set: None,
            npc_scenes: HashMap::new(),
            wheelbarrow_scene: None,
            wheelbarrow_frames: None,
            wheelbarrow_overlay_scale: Vector2::ONE,
            npc_sprites: HashMap::new(),
            building_textures: HashMap::new(),
            building_sprites: HashMap::new(),
            building_shadows: HashMap::new(),
            tick_performance_sample: None,
            tick_performance_sequence: 0,
            active_renderer_mode: RendererMode::TwoD,
            surface_snapshot: None,
            dynamic_snapshot: snapshot::DynamicRenderSnapshot::default(),
            overlay_snapshot: snapshot::WorldOverlaySnapshot::default(),
            snapshot_revision: 0,
            renderer_2d_revision: 0,
            renderer_3d_revision: 0,
            surface_generation: 0,
            renderer_2d_surface_generation: 0,
            orbit_previous_mouse_mode: None,
            base,
        }
    }

    fn ready(&mut self) {
        let (
            mut tm,
            mut resource_map,
            mut crop_map,
            mut tree_plot_map,
            mut road_map,
            mut road_blueprint_map,
            mut cam,
        ) = {
            let renderer = self.renderer_2d.bind();
            (
                renderer.tile_map(),
                renderer.resource_node_map(),
                renderer.crop_map(),
                renderer.tree_plot_map(),
                renderer.road_map(),
                renderer.road_blueprint_map(),
                renderer.camera(),
            )
        };

        if !self.refresh_surface_snapshot() {
            self.disable_processing();
            return;
        }
        self.refresh_dynamic_snapshot();

        debug_assert_eq!(grid::TILE_SIZE as i32, LOGICAL_TILE_SIZE);
        let Some(terrain) = self.build_terrain_tile_set() else {
            self.disable_processing();
            return;
        };

        tm.set_tile_set(&terrain.tile_set);
        tm.set_scale(terrain.metrics.node_scale());
        self._tile_set = Some(terrain.tile_set);
        tm.set_navigation_enabled(false);
        tm.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
        tm.set_draw_behind_parent(true);

        if !self.populate_tile_map(&mut tm) {
            self.disable_processing();
            return;
        }

        let Some(roads) = self.build_road_tile_set() else {
            self.disable_processing();
            return;
        };
        road_map.set_tile_set(&roads.tile_set);
        road_blueprint_map.set_tile_set(&roads.tile_set);
        self._road_tile_set = Some(roads.tile_set);
        for map in [&mut road_map, &mut road_blueprint_map] {
            map.set_scale(roads.metrics.node_scale());
            map.set_navigation_enabled(false);
            map.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
        }
        road_map.set_z_index(1);
        road_blueprint_map.set_z_index(2);
        road_blueprint_map.set_modulate(Color::from_rgba(0.15, 0.85, 1.0, 0.72));
        self.populate_road_maps(&mut road_map, &mut road_blueprint_map);

        if !self.load_building_textures() {
            self.disable_processing();
            return;
        }
        self.sync_building_sprites();

        let Some(resources) = self.build_resource_node_tile_set() else {
            self.disable_processing();
            return;
        };
        resource_map.set_tile_set(&resources.tile_set);
        resource_map.set_scale(resources.metrics.node_scale());
        self._resource_node_tile_set = Some(resources.tile_set);
        resource_map.set_navigation_enabled(false);
        resource_map.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
        resource_map.set_z_index(3);
        self.populate_resource_node_map(&mut resource_map);

        let Some(crops) = self.build_crop_tile_set() else {
            self.disable_processing();
            return;
        };
        crop_map.set_tile_set(&crops.tile_set);
        crop_map.set_scale(crops.metrics.node_scale());
        self._crop_tile_set = Some(crops.tile_set);
        crop_map.set_navigation_enabled(false);
        crop_map.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
        crop_map.set_z_index(4);
        self.populate_crop_map(&mut crop_map);

        let Some(tree_plots) = self.build_tree_plot_tile_set() else {
            self.disable_processing();
            return;
        };
        tree_plot_map.set_tile_set(&tree_plots.tile_set);
        tree_plot_map.set_scale(tree_plots.metrics.node_scale());
        self._tree_plot_tile_set = Some(tree_plots.tile_set);
        tree_plot_map.set_navigation_enabled(false);
        tree_plot_map.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
        tree_plot_map.set_z_index(4);
        self.populate_tree_plot_map(&mut tree_plot_map);

        if !self.load_npc_scenes() {
            self.disable_processing();
            return;
        }
        if !self.load_wheelbarrow_scene() {
            self.disable_processing();
            return;
        }
        self.sync_npc_sprites();

        cam.set_enabled(true);
        cam.make_current();
        cam.set_zoom(Vector2::new(0.5, 0.5));
        self.configure_camera_for_surface();
        if let Some(surface) = self.surface_snapshot.as_ref() {
            self.renderer_3d.bind_mut().configure_surface(
                surface.size.width_i32().unwrap_or(1),
                surface.size.height_i32().unwrap_or(1),
                true,
            );
        }

        self.renderer_2d.bind_mut().set_active(true);
        self.renderer_3d.bind_mut().set_active(false);
        self.refresh_overlay_snapshot();
        self.sync_renderer_2d_snapshots();

        self.base_mut().set_process_input(true);
        self.base_mut().set_process(true);
        self.queue_renderer_redraw();
    }

    fn process(&mut self, delta: f64) {
        let availability = self.renderer_3d.bind().availability();
        let safe_mode =
            renderer_mode_after_availability_check(self.active_renderer_mode, availability);
        if safe_mode != self.active_renderer_mode {
            self.set_renderer_mode(safe_mode);
        }
        if self.active_renderer_mode == RendererMode::TwoD
            && self.renderer_3d.bind().availability() == Renderer3DAvailability::Preparing
        {
            self.renderer_3d.bind_mut().prewarm_step();
        }
        let input = Input::singleton();

        let mut dir = Vector2::ZERO;
        if input.is_action_pressed(ACTION_CAMERA_PAN_UP) {
            dir.y -= 1.0;
        }
        if input.is_action_pressed(ACTION_CAMERA_PAN_DOWN) {
            dir.y += 1.0;
        }
        if input.is_action_pressed(ACTION_CAMERA_PAN_LEFT) {
            dir.x -= 1.0;
        }
        if input.is_action_pressed(ACTION_CAMERA_PAN_RIGHT) {
            dir.x += 1.0;
        }

        match self.active_renderer_mode {
            RendererMode::TwoD => {
                let mut cam = self.camera_2d();
                let vs = self.get_viewport_size();
                let ws = self.world_size();
                let min_zoom = {
                    let fit_x = vs.x / ws.x;
                    let fit_y = vs.y / ws.y;
                    (fit_x.max(fit_y) * ZOOM_MARGIN).max(ZOOM_ABSOLUTE_FLOOR)
                };
                let zoom = cam.get_zoom().x;
                let clamped = zoom.clamp(min_zoom, ZOOM_MAX);
                if (clamped - zoom).abs() > f32::EPSILON {
                    cam.set_zoom(Vector2::new(clamped, clamped));
                }
                if dir != Vector2::ZERO {
                    let direction = dir.normalized();
                    let speed = PAN_SPEED / cam.get_zoom().x;
                    let pos = cam.get_position();
                    cam.set_position(pos + direction * speed * delta as f32);
                }
            }
            RendererMode::ThreeD => {
                self.renderer_3d.bind_mut().pan(dir, delta);
            }
        }

        let fixed_tick_count = if self.game.is_playing() {
            u64::from(self.game.simulation_speed().multiplier())
        } else {
            0
        };
        let tick_started = Instant::now();
        self.game.tick();
        self.tick_performance_sequence = self.tick_performance_sequence.wrapping_add(1);
        self.tick_performance_sample = Some(TickPerformanceSample {
            sequence: self.tick_performance_sequence,
            wall_time: tick_started.elapsed(),
            fixed_tick_count,
        });
        self.refresh_dynamic_snapshot();
        self.reconcile_selection_from_snapshot();
        self.sync_selected_npc_route_overlay();
        self.update_drag_current();
        self.refresh_overlay_snapshot();
        match self.active_renderer_mode {
            RendererMode::TwoD => self.sync_renderer_2d_snapshots(),
            RendererMode::ThreeD => self.sync_renderer_3d_snapshots(),
        }
        self.update_hovered_map_entity();
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if event.is_action_pressed(ACTION_MENU_TOGGLE) && self.placement_mode.is_some() {
            self.cancel_placement_mode();
            self.mark_input_handled();
            return;
        }

        if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            if self.active_renderer_mode == RendererMode::ThreeD
                && self.renderer_3d.bind().is_orbiting()
            {
                self.renderer_3d.bind_mut().orbit(motion.get_relative());
                self.clear_hovered_map_entity();
                self.mark_input_handled();
            }
            return;
        }

        let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() else {
            return;
        };

        if mouse.is_pressed() && self.pointer_is_over_construction_dock() {
            return;
        }

        match mouse.get_button_index() {
            MouseButton::LEFT => {
                if mouse.is_pressed() {
                    self.handle_primary_press();
                } else {
                    self.handle_primary_release();
                }
            }
            MouseButton::RIGHT => {
                if mouse.is_pressed() {
                    if self.placement_mode.is_some() {
                        self.cancel_placement_mode();
                        self.mark_input_handled();
                    } else if self.handle_building_context_click() {
                        self.mark_input_handled();
                    }
                }
            }
            MouseButton::MIDDLE => {
                if self.active_renderer_mode == RendererMode::ThreeD {
                    if mouse.is_pressed() {
                        self.begin_3d_orbit_capture();
                    } else {
                        self.end_3d_orbit_capture();
                    }
                    self.mark_input_handled();
                }
            }
            MouseButton::WHEEL_UP => {
                if mouse.is_pressed() {
                    self.handle_mouse_wheel(ZOOM_FACTOR);
                }
            }
            MouseButton::WHEEL_DOWN => {
                if mouse.is_pressed() {
                    self.handle_mouse_wheel(1.0 / ZOOM_FACTOR);
                }
            }
            _ => {}
        }
    }
}

impl GameWorld {
    pub(crate) const fn tick_performance_sample(&self) -> Option<TickPerformanceSample> {
        self.tick_performance_sample
    }

    fn camera_2d(&self) -> Gd<Camera2D> {
        self.renderer_2d.bind().camera()
    }

    fn resource_node_map_2d(&self) -> Gd<TileMapLayer> {
        self.renderer_2d.bind().resource_node_map()
    }

    fn crop_map_2d(&self) -> Gd<TileMapLayer> {
        self.renderer_2d.bind().crop_map()
    }

    fn tree_plot_map_2d(&self) -> Gd<TileMapLayer> {
        self.renderer_2d.bind().tree_plot_map()
    }

    fn road_maps_2d(&self) -> (Gd<TileMapLayer>, Gd<TileMapLayer>) {
        let renderer = self.renderer_2d.bind();
        (renderer.road_map(), renderer.road_blueprint_map())
    }

    fn pointer_world_position(&self) -> Vector2 {
        match self.active_renderer_mode {
            RendererMode::TwoD => self.renderer_2d.bind().local_mouse_position(),
            RendererMode::ThreeD => {
                let screen_position = self
                    .base()
                    .get_viewport()
                    .map(|viewport| viewport.get_mouse_position())
                    .unwrap_or(Vector2::ZERO);
                self.renderer_3d
                    .bind()
                    .pointer_ray(screen_position)
                    .ground_intersection()
                    .map(|point| {
                        Vector2::new(point.x, point.z) * (grid::TILE_SIZE / WORLD_UNITS_PER_TILE)
                    })
                    .unwrap_or(Vector2::new(f32::NAN, f32::NAN))
            }
        }
    }

    fn queue_renderer_redraw(&mut self) {
        self.renderer_2d.bind_mut().queue_overlay_redraw();
    }

    fn begin_3d_orbit_capture(&mut self) {
        if self.renderer_3d.bind().is_orbiting() {
            return;
        }
        let mut input = Input::singleton();
        self.orbit_previous_mouse_mode = Some(input.get_mouse_mode());
        input.set_mouse_mode(input::MouseMode::CAPTURED);
        self.renderer_3d.bind_mut().begin_orbit();
        self.clear_hovered_map_entity();
    }

    fn end_3d_orbit_capture(&mut self) {
        self.renderer_3d.bind_mut().end_orbit();
        if let Some(mouse_mode) = self.orbit_previous_mouse_mode.take() {
            Input::singleton().set_mouse_mode(mouse_mode);
        }
    }

    fn add_2d_child<T>(&mut self, child: &Gd<T>)
    where
        T: GodotClass + Inherits<Node>,
    {
        let mut renderer = self.renderer_2d.clone();
        renderer.add_child(child);
    }

    fn refresh_surface_snapshot(&mut self) -> bool {
        let size = self.grid_size();
        let mut terrain_cells = Vec::with_capacity(size.cell_count().unwrap_or_default());
        for coord in self.game.tile_coords(self.rendered_surface) {
            let Some(kind) = self.game.tile_terrain_at(self.rendered_surface, coord) else {
                godot_error!(
                    "GameWorld: terrain missing for tile at ({}, {})",
                    coord.x(),
                    coord.y()
                );
                return false;
            };
            terrain_cells.push(snapshot::TerrainRenderCell {
                coord,
                kind,
                variant: terrain_variant(self.rendered_surface, coord, kind),
            });
        }
        self.surface_snapshot = Some(snapshot::SurfaceRenderSnapshot::new(
            self.rendered_surface,
            size,
            terrain_cells,
        ));
        self.surface_generation = self.surface_generation.wrapping_add(1);
        true
    }

    fn refresh_dynamic_snapshot(&mut self) {
        self.dynamic_snapshot =
            self.with_rendered_surface_world(snapshot::DynamicRenderSnapshot::from_world);
        self.snapshot_revision = self.snapshot_revision.wrapping_add(1);
    }

    fn refresh_overlay_snapshot(&mut self) {
        let selected_npc = self.selected_npc.and_then(|selected| {
            self.dynamic_snapshot
                .npcs
                .iter()
                .find(|npc| npc.entity == selected.entity)
                .map(|npc| snapshot::SelectedNpcOverlay {
                    entity: selected.entity,
                    position: npc.position,
                })
        });
        let selected_npc_route = self
            .selected_npc_route_overlay
            .clone()
            .map(|route| match route {
                SelectedNpcRouteOverlay::Route {
                    position,
                    waypoints,
                    destination,
                } => snapshot::NpcRouteOverlay::Route {
                    position,
                    waypoints,
                    destination,
                },
                SelectedNpcRouteOverlay::Blocked { position } => {
                    snapshot::NpcRouteOverlay::Blocked { position }
                }
            });
        self.overlay_snapshot = snapshot::WorldOverlaySnapshot {
            selected_cell: self
                .selected_cell
                .map(|selected| snapshot::SelectedCellOverlay {
                    entity: selected.entity,
                    coord: selected.coord,
                }),
            selected_npc,
            selected_building: self.selected_building.map(|selected| {
                snapshot::SelectedBuildingOverlay {
                    entity: selected.entity,
                    footprint: selected.footprint,
                }
            }),
            selected_npc_route,
            building_preview: self.building_preview().map(|(kind, footprint, valid)| {
                snapshot::BuildingPreviewOverlay {
                    kind,
                    footprint,
                    validity: valid.into(),
                }
            }),
            plot_cells: self
                .plot_previews()
                .into_iter()
                .map(|preview| snapshot::PlacementCellOverlay {
                    coord: preview.coord,
                    validity: preview.valid.into(),
                })
                .collect(),
            road_cells: self
                .road_previews()
                .into_iter()
                .map(|(coord, valid)| snapshot::PlacementCellOverlay {
                    coord,
                    validity: valid.into(),
                })
                .collect(),
        };
    }

    fn sync_renderer_2d_snapshots(&mut self) {
        if self.renderer_2d_surface_generation != self.surface_generation {
            let mut tile_map = self.renderer_2d.bind().tile_map();
            if !self.populate_tile_map(&mut tile_map) {
                self.disable_processing();
                return;
            }
            self.renderer_2d_surface_generation = self.surface_generation;
        }
        if self.renderer_2d_revision != self.snapshot_revision {
            let mut resource_map = self.resource_node_map_2d();
            self.populate_resource_node_map(&mut resource_map);
            let mut crop_map = self.crop_map_2d();
            self.populate_crop_map(&mut crop_map);
            let mut tree_plot_map = self.tree_plot_map_2d();
            self.populate_tree_plot_map(&mut tree_plot_map);
            let (mut road_map, mut road_blueprint_map) = self.road_maps_2d();
            self.populate_road_maps(&mut road_map, &mut road_blueprint_map);
            self.sync_building_sprites();
            self.sync_npc_sprites();
        }
        let mut renderer = self.renderer_2d.clone();
        let surface = self.surface_snapshot.as_ref();
        let dynamic = &self.dynamic_snapshot;
        let overlay = &self.overlay_snapshot;
        let mut renderer = renderer.bind_mut();
        if let Some(surface) = surface {
            renderer.apply_surface_snapshot(surface);
        }
        renderer.apply_dynamic_snapshot(dynamic);
        renderer.apply_overlay_snapshot(overlay);
        self.renderer_2d_revision = self.snapshot_revision;
    }

    fn sync_renderer_3d_snapshots(&mut self) {
        let mut renderer = self.renderer_3d.clone();
        let surface = self.surface_snapshot.as_ref();
        let dynamic = &self.dynamic_snapshot;
        let overlay = &self.overlay_snapshot;
        let mut renderer = renderer.bind_mut();
        if let Some(surface) = surface {
            renderer.apply_surface_snapshot(surface);
        }
        renderer.apply_dynamic_snapshot(dynamic);
        renderer.apply_overlay_snapshot(overlay);
        self.renderer_3d_revision = self.snapshot_revision;
    }

    fn reconcile_selection_from_snapshot(&mut self) {
        if let Some(selected) = self.selected_npc {
            if let Some(npc) = self
                .dynamic_snapshot
                .npcs
                .iter()
                .find(|npc| npc.entity == selected.entity)
            {
                self.selected_npc = Some(SelectedNpc {
                    coord: npc.position.coord,
                    entity: selected.entity,
                });
            } else {
                self.clear_npc_selection();
            }
        }
        if let Some(selected) = self.selected_building {
            let exists = self
                .dynamic_snapshot
                .buildings
                .iter()
                .any(|building| building.entity == selected.entity);
            if !exists {
                self.clear_building_selection();
            }
        }
    }

    fn disable_processing(&mut self) {
        self.base_mut().set_process_input(false);
        self.base_mut().set_process(false);
    }

    fn get_viewport_size(&self) -> Vector2 {
        self.base()
            .get_viewport()
            .map(|vp| vp.get_visible_rect().size)
            .unwrap_or_else(|| Vector2::new(1920.0, 1080.0))
    }

    fn world_size(&self) -> Vector2 {
        let size = self.grid_size();
        Vector2::new(
            size.width() as f32 * grid::TILE_SIZE,
            size.height() as f32 * grid::TILE_SIZE,
        )
    }

    fn populate_tile_map(&self, tile_map: &mut Gd<TileMapLayer>) -> bool {
        tile_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let Some(snapshot) = self.surface_snapshot.as_ref() else {
            godot_error!("GameWorld: surface snapshot unavailable");
            return false;
        };
        for cell in &snapshot.terrain_cells {
            tile_map
                .set_cell_ex(v2(cell.coord.x(), cell.coord.y()))
                .source_id(terrain_source_id(cell.kind))
                .atlas_coords(v2(cell.variant, 0))
                .done();
        }
        tile_map.update_internals();
        true
    }

    fn configure_camera_for_surface(&mut self) {
        let world_size = self.world_size();
        self.renderer_2d
            .bind_mut()
            .configure_for_world_size(world_size);
    }

    fn handle_primary_press(&mut self) {
        match self.placement_mode.as_ref() {
            Some(PlacementMode::Building(kind)) => {
                self.handle_build_click(*kind);
            }
            Some(PlacementMode::Plots { .. }) => {
                self.begin_plot_drag();
            }
            Some(PlacementMode::Roads { .. }) => {
                self.begin_road_drag();
            }
            None => {
                self.handle_tile_click();
            }
        }
    }

    fn handle_primary_release(&mut self) {
        if matches!(self.placement_mode, Some(PlacementMode::Plots { .. })) {
            self.finish_plot_drag();
        } else if matches!(self.placement_mode, Some(PlacementMode::Roads { .. })) {
            self.finish_road_drag();
        }
    }

    fn handle_build_click(&mut self, kind: BuildingKind) {
        let Some(origin) = self.placement_origin_under_mouse() else {
            return;
        };

        match self
            .game
            .place_building_blueprint(self.rendered_surface, kind, origin)
        {
            Ok(_) => {
                self.sync_building_sprites();
                self.queue_renderer_redraw();
            }
            Err(error) => {
                godot_warn!("GameWorld: building placement rejected: {error:?}");
            }
        }
    }

    fn handle_tile_click(&mut self) {
        let mouse_pos = self.pointer_world_position();
        let ground_coord = Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        );
        let proxy_target = self.valid_3d_proxy_target_under_mouse();

        if ground_coord.is_some()
            || matches!(
                proxy_target,
                Some(MapEntityTarget {
                    kind: MapEntityKind::Npc,
                    ..
                })
            )
        {
            let mut targets = self.with_rendered_surface_world(|world| {
                let mut targets = ground_coord
                    .map(|coord| click_selection_targets_at(world, coord))
                    .unwrap_or_default();

                if let Some(MapEntityTarget {
                    kind: MapEntityKind::Npc,
                    entity,
                }) = proxy_target
                {
                    if let Some(npc) = selected_npc_for_entity(world, entity) {
                        if targets.tile.is_none() {
                            targets.tile = selected_cell_at(world, npc.coord);
                        }
                        targets.npc = Some(npc);
                    }
                }
                targets
            });
            if targets.tile.is_none() {
                godot_error!("GameWorld: selected tile entity unavailable");
                self.disable_processing();
                return;
            }
            targets.building = None;

            let mut ignored_building_selection = None;
            let events = apply_click_selection_targets(
                &mut self.selected_cell,
                &mut self.selected_npc,
                &mut ignored_building_selection,
                targets,
            );
            self.sync_selected_npc_route_overlay();
            self.queue_renderer_redraw();
            self.emit_selection_events(events);
        } else {
            self.clear_tile_selection();
            self.clear_npc_selection();
        }
    }

    fn handle_building_context_click(&mut self) -> bool {
        if let Some(building) = self.valid_3d_building_proxy_under_mouse() {
            return self.select_building_context(building);
        }

        let mouse_pos = self.pointer_world_position();
        let Some(coord) = Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        ) else {
            return false;
        };
        let Some(building) =
            self.with_rendered_surface_world(|world| selected_building_at(world, coord))
        else {
            return false;
        };
        self.select_building_context(building)
    }

    fn select_building_context(&mut self, building: SelectedBuilding) -> bool {
        self.selected_building = Some(building);
        self.queue_renderer_redraw();
        let Ok(entity_id) = BridgeEntityId::try_from(building.entity) else {
            godot_error!("GameWorld: building context entity id is too large for Godot");
            return false;
        };
        self.signals()
            .building_selected()
            .emit(entity_id.signal_value());
        true
    }

    fn handle_mouse_wheel(&mut self, factor: f32) {
        if self.active_renderer_mode == RendererMode::ThreeD {
            self.renderer_3d.bind_mut().dolly(factor);
            return;
        }
        let mut cam = self.camera_2d();
        let old_zoom = cam.get_zoom().x;

        let vs = self.get_viewport_size();
        let ws = self.world_size();
        let min_zoom = {
            let fit_x = vs.x / ws.x;
            let fit_y = vs.y / ws.y;
            (fit_x.max(fit_y) * ZOOM_MARGIN).max(ZOOM_ABSOLUTE_FLOOR)
        };
        let new_zoom = (old_zoom * factor).clamp(min_zoom, ZOOM_MAX);
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            return;
        }

        let viewport = self.base().get_viewport();
        let mouse_pos = viewport
            .map(|vp| vp.get_mouse_position())
            .unwrap_or(Vector2::ZERO);
        let half_vs = vs / 2.0;
        let cursor_offset = mouse_pos - half_vs;

        let world_under_cursor = cam.get_position() + cursor_offset / old_zoom;

        cam.set_zoom(Vector2::new(new_zoom, new_zoom));
        cam.set_position(world_under_cursor - cursor_offset / new_zoom);
    }

    fn clear_tile_selection(&mut self) {
        if self.selected_cell.take().is_some() {
            self.queue_renderer_redraw();
            self.signals().tile_deselected().emit();
        }
    }

    fn clear_npc_selection(&mut self) {
        if self.selected_npc.take().is_some() {
            self.selected_npc_route_overlay = None;
            self.queue_renderer_redraw();
            self.signals().npc_deselected().emit();
        }
    }

    fn clear_building_selection(&mut self) {
        if self.selected_building.take().is_some() {
            self.queue_renderer_redraw();
            self.signals().building_deselected().emit();
        }
    }

    fn emit_selection_events(&mut self, events: Vec<SelectionEvent>) {
        for event in events {
            match event {
                SelectionEvent::TileSelected(entity) => {
                    let Ok(tile_entity_id) = BridgeEntityId::try_from(entity) else {
                        godot_error!("GameWorld: selected tile entity id is too large for Godot");
                        continue;
                    };
                    self.signals()
                        .tile_selected()
                        .emit(tile_entity_id.signal_value());
                }
                SelectionEvent::TileDeselected => {
                    self.signals().tile_deselected().emit();
                }
                SelectionEvent::NpcSelected(entity) => {
                    let Ok(npc_entity_id) = BridgeEntityId::try_from(entity) else {
                        godot_error!("GameWorld: selected NPC entity id is too large for Godot");
                        continue;
                    };
                    self.signals()
                        .npc_selected()
                        .emit(npc_entity_id.signal_value());
                }
                SelectionEvent::NpcDeselected => {
                    self.signals().npc_deselected().emit();
                }
                SelectionEvent::BuildingSelected(entity) => {
                    let Ok(building_entity_id) = BridgeEntityId::try_from(entity) else {
                        godot_error!(
                            "GameWorld: selected building entity id is too large for Godot"
                        );
                        continue;
                    };
                    self.signals()
                        .building_selected()
                        .emit(building_entity_id.signal_value());
                }
                SelectionEvent::BuildingDeselected => {
                    self.signals().building_deselected().emit();
                }
            }
        }
    }

    fn update_hovered_map_entity(&mut self) {
        let target = self.map_entity_target_under_mouse();
        self.set_hovered_map_entity(target);
    }

    fn clear_hovered_map_entity(&mut self) {
        self.set_hovered_map_entity(None);
    }

    fn set_hovered_map_entity(&mut self, target: Option<MapEntityTarget>) {
        if self.hovered_map_entity == target {
            return;
        }

        self.hovered_map_entity = target;
        let Some(target) = target else {
            self.signals().map_entity_unhovered().emit();
            return;
        };

        let Ok(entity_id) = BridgeEntityId::try_from(target.entity) else {
            godot_error!("GameWorld: hovered map entity id is too large for Godot");
            self.hovered_map_entity = None;
            self.signals().map_entity_unhovered().emit();
            return;
        };

        self.signals()
            .map_entity_hovered()
            .emit(target.kind.signal_value(), entity_id.signal_value());
    }

    fn start_build_mode(&mut self, kind: BuildingKind) {
        self.placement_mode = Some(PlacementMode::Building(kind));
        self.clear_tile_selection();
        self.clear_npc_selection();
        self.clear_building_selection();
        self.clear_hovered_map_entity();
        self.queue_renderer_redraw();
    }

    fn start_plot_placement_mode(&mut self, owner: PlotOwner) {
        self.placement_mode = Some(PlacementMode::Plots {
            owner,
            drag_cells: Vec::new(),
        });
        self.clear_tile_selection();
        self.clear_npc_selection();
        self.clear_building_selection();
        self.clear_hovered_map_entity();
        self.queue_renderer_redraw();
    }

    fn start_road_placement_mode(&mut self, tier: RoadTier) {
        self.placement_mode = Some(PlacementMode::Roads {
            tier,
            drag_cells: Vec::new(),
            last_rejection: None,
        });
        self.clear_tile_selection();
        self.clear_npc_selection();
        self.clear_building_selection();
        self.clear_hovered_map_entity();
        self.queue_renderer_redraw();
    }

    fn cancel_placement_mode(&mut self) {
        if self.placement_mode.take().is_some() {
            self.queue_renderer_redraw();
        }
    }

    fn mark_input_handled(&self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }

    fn pointer_is_over_construction_dock(&self) -> bool {
        let tree = self.base().get_tree();
        let root = tree.get_root();
        let Some(hovered) = root.gui_get_hovered_control() else {
            return false;
        };

        let mut node = Some(hovered.upcast::<Node>());
        while let Some(current) = node {
            if current.clone().try_cast::<ConstructionDock>().is_ok() {
                return true;
            }
            node = current.get_parent();
        }
        false
    }

    fn placement_origin_under_mouse(&self) -> Option<CellCoord> {
        let mouse_pos = self.pointer_world_position();
        Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        )
    }

    fn building_preview(&self) -> Option<(BuildingKind, BuildingFootprint, bool)> {
        let Some(PlacementMode::Building(kind)) = self.placement_mode.as_ref() else {
            return None;
        };
        let origin = self.placement_origin_under_mouse()?;
        let definition = kind.definition();
        let footprint = BuildingFootprint::new(origin, definition.width(), definition.height());
        let valid = self
            .game
            .validate_building_blueprint_placement(self.rendered_surface, *kind, origin)
            .is_ok();

        Some((*kind, footprint, valid))
    }

    fn plot_previews(&self) -> Vec<PlotPlacementPreview> {
        let Some(PlacementMode::Plots { owner, drag_cells }) = self.placement_mode.as_ref() else {
            return Vec::new();
        };

        let coords = if drag_cells.is_empty() {
            self.placement_origin_under_mouse()
                .map(|coord| vec![coord])
                .unwrap_or_default()
        } else {
            drag_cells.clone()
        };

        match owner {
            PlotOwner::Farm(farm) => self
                .game
                .validate_field_blueprint_placement_batch(self.rendered_surface, *farm, coords)
                .into_iter()
                .map(|preview| PlotPlacementPreview {
                    coord: preview.coord,
                    valid: preview.result.is_ok(),
                })
                .collect(),
            PlotOwner::ForesterLodge(lodge) => self
                .game
                .validate_tree_plot_blueprint_placement_batch(self.rendered_surface, *lodge, coords)
                .into_iter()
                .map(|preview| PlotPlacementPreview {
                    coord: preview.coord,
                    valid: preview.result.is_ok(),
                })
                .collect(),
        }
    }

    fn road_validation(&self) -> Option<RoadPlacementBatchResult> {
        let PlacementMode::Roads {
            tier,
            drag_cells,
            last_rejection,
        } = self.placement_mode.as_ref()?
        else {
            return None;
        };
        if drag_cells.is_empty() {
            if let Some(rejection) = last_rejection {
                return Some(rejection.clone());
            }
        }
        let coords = if drag_cells.is_empty() {
            self.placement_origin_under_mouse()
                .map(|coord| vec![coord])
                .unwrap_or_default()
        } else {
            drag_cells.clone()
        };
        Some(
            self.game
                .validate_road_placement_batch(self.rendered_surface, *tier, coords),
        )
    }

    fn road_previews(&self) -> Vec<(CellCoord, bool)> {
        self.road_validation()
            .map(|validation| {
                validation
                    .cells
                    .into_iter()
                    .map(|cell| (cell.coord, cell.is_valid()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn begin_plot_drag(&mut self) {
        let Some(coord) = self.placement_origin_under_mouse() else {
            return;
        };
        if let Some(PlacementMode::Plots { drag_cells, .. }) = &mut self.placement_mode {
            drag_cells.clear();
            append_plot_drag_cell(drag_cells, Some(coord));
            self.queue_renderer_redraw();
        }
    }

    fn update_drag_current(&mut self) {
        let coord = self.placement_origin_under_mouse();
        if let Some(PlacementMode::Plots { drag_cells, .. }) = &mut self.placement_mode {
            if !drag_cells.is_empty() {
                let before_len = drag_cells.len();
                append_plot_drag_cell(drag_cells, coord);
                if drag_cells.len() != before_len {
                    self.queue_renderer_redraw();
                }
            }
        }
        if let Some(PlacementMode::Roads { drag_cells, .. }) = &mut self.placement_mode {
            if !drag_cells.is_empty() {
                let before_len = drag_cells.len();
                append_road_drag_cell(drag_cells, coord);
                if drag_cells.len() != before_len {
                    self.queue_renderer_redraw();
                }
            }
        }
    }

    fn begin_road_drag(&mut self) {
        let Some(coord) = self.placement_origin_under_mouse() else {
            return;
        };
        if let Some(PlacementMode::Roads {
            drag_cells,
            last_rejection,
            ..
        }) = &mut self.placement_mode
        {
            drag_cells.clear();
            drag_cells.push(coord);
            *last_rejection = None;
            self.queue_renderer_redraw();
        }
    }

    fn finish_road_drag(&mut self) {
        let Some(PlacementMode::Roads {
            tier, drag_cells, ..
        }) = self.placement_mode.clone()
        else {
            return;
        };
        if drag_cells.is_empty() {
            return;
        }
        let result = self
            .game
            .place_road_blueprints(self.rendered_surface, tier, drag_cells);
        let last_rejection = match result {
            Ok(_) => None,
            Err(validation) => {
                for cell in &validation.cells {
                    for error in &cell.errors {
                        godot_warn!(
                            "GameWorld: road placement rejected at ({}, {}): {}",
                            cell.coord.x(),
                            cell.coord.y(),
                            error.label()
                        );
                    }
                }
                Some(validation)
            }
        };
        self.placement_mode = Some(PlacementMode::Roads {
            tier,
            drag_cells: Vec::new(),
            last_rejection,
        });
        self.queue_renderer_redraw();
    }

    fn finish_plot_drag(&mut self) {
        let Some(PlacementMode::Plots { owner, drag_cells }) = self.placement_mode.clone() else {
            return;
        };

        if drag_cells.is_empty() {
            self.placement_mode = Some(PlacementMode::Plots {
                owner,
                drag_cells: Vec::new(),
            });
            self.queue_renderer_redraw();
            return;
        }

        let placed_any = match owner {
            PlotOwner::Farm(farm) => {
                let result =
                    self.game
                        .place_field_blueprints(self.rendered_surface, farm, drag_cells);
                for rejected in &result.rejected {
                    godot_warn!(
                        "GameWorld: field placement rejected at ({}, {}): {:?}",
                        rejected.coord.x(),
                        rejected.coord.y(),
                        rejected.error
                    );
                }
                !result.placed.is_empty()
            }
            PlotOwner::ForesterLodge(lodge) => {
                let result =
                    self.game
                        .place_tree_plot_blueprints(self.rendered_surface, lodge, drag_cells);
                for rejected in &result.rejected {
                    godot_warn!(
                        "GameWorld: tree plot placement rejected at ({}, {}): {:?}",
                        rejected.coord.x(),
                        rejected.coord.y(),
                        rejected.error
                    );
                }
                !result.placed.is_empty()
            }
        };

        if placed_any {
            self.sync_building_sprites();
        }
        self.placement_mode = Some(PlacementMode::Plots {
            owner,
            drag_cells: Vec::new(),
        });
        self.queue_renderer_redraw();
    }

    fn map_entity_target_under_mouse(&self) -> Option<MapEntityTarget> {
        if self.placement_mode.is_some()
            || (self.active_renderer_mode == RendererMode::ThreeD
                && self.renderer_3d.bind().is_orbiting())
        {
            return None;
        }

        let viewport = self.base().get_viewport()?;
        let mouse_pos = viewport.get_mouse_position();
        let viewport_size = self.get_viewport_size();
        if mouse_pos.x < 0.0
            || mouse_pos.y < 0.0
            || mouse_pos.x >= viewport_size.x
            || mouse_pos.y >= viewport_size.y
        {
            return None;
        }

        if let Some(target) = self.valid_3d_proxy_target_at(mouse_pos) {
            return Some(target);
        }

        let local_mouse_pos = self.pointer_world_position();
        let coord = Grid::world_to_cell(
            WorldPosition::new(local_mouse_pos.x, local_mouse_pos.y),
            self.grid_size(),
        )?;

        self.with_rendered_surface_world(|world| map_entity_target_at(world, coord))
    }

    fn proxy_hits_at(&self, screen_position: Vector2) -> Vec<ProxyHit3D> {
        if self.active_renderer_mode != RendererMode::ThreeD {
            return Vec::new();
        }

        self.renderer_3d.bind().proxy_hits(screen_position)
    }

    fn valid_3d_proxy_target_under_mouse(&self) -> Option<MapEntityTarget> {
        if self.active_renderer_mode != RendererMode::ThreeD {
            return None;
        }

        let viewport = self.base().get_viewport()?;
        let mouse_pos = viewport.get_mouse_position();
        let viewport_size = self.get_viewport_size();
        if mouse_pos.x < 0.0
            || mouse_pos.y < 0.0
            || mouse_pos.x >= viewport_size.x
            || mouse_pos.y >= viewport_size.y
        {
            return None;
        }

        self.valid_3d_proxy_target_at(mouse_pos)
    }

    fn valid_3d_proxy_target_at(&self, screen_position: Vector2) -> Option<MapEntityTarget> {
        let hits = self.proxy_hits_at(screen_position);
        self.with_rendered_surface_world(|world| {
            hits.into_iter()
                .find_map(|hit| map_entity_target_from_proxy(world, hit))
        })
    }

    fn valid_3d_building_proxy_under_mouse(&self) -> Option<SelectedBuilding> {
        if self.active_renderer_mode != RendererMode::ThreeD {
            return None;
        }

        let viewport = self.base().get_viewport()?;
        let mouse_pos = viewport.get_mouse_position();
        let viewport_size = self.get_viewport_size();
        if mouse_pos.x < 0.0
            || mouse_pos.y < 0.0
            || mouse_pos.x >= viewport_size.x
            || mouse_pos.y >= viewport_size.y
        {
            return None;
        }

        let hits = self.proxy_hits_at(mouse_pos);
        self.with_rendered_surface_world(|world| {
            hits.into_iter()
                .filter(|hit| hit.kind == ProxyKind3D::Building)
                .find_map(|hit| selected_building_for_entity(world, hit.entity))
        })
    }

    fn grid_size(&self) -> game_engine::grid::GridSize {
        self.game.grid_size(self.rendered_surface)
    }

    pub(crate) fn with_rendered_surface_world<R>(&self, f: impl FnOnce(&World) -> R) -> R {
        self.game.with_surface_world(self.rendered_surface, f)
    }

    pub(crate) fn with_rendered_surface_resource_overview<R>(
        &mut self,
        f: impl FnOnce(ResourceOverview, &World) -> R,
    ) -> R {
        self.game
            .with_surface_resource_overview(self.rendered_surface, f)
    }

    pub(crate) fn simulation_datetime_text_string(&self) -> String {
        let date_time = self.game.world_date_time();
        format!(
            "Day {} {:02}:{:02}",
            date_time.day(),
            date_time.hour(),
            date_time.minute()
        )
    }

    fn sync_building_sprites(&mut self) -> bool {
        let buildings = self.building_render_infos();
        let mut redraw_needed = false;

        let active_entities: HashSet<Entity> =
            buildings.iter().map(|building| building.entity).collect();
        let stale_entities: Vec<Entity> = self
            .building_sprites
            .keys()
            .copied()
            .filter(|entity| !active_entities.contains(entity))
            .collect();
        for entity in stale_entities {
            if let Some(mut sprite) = self.building_sprites.remove(&entity) {
                sprite.queue_free();
                redraw_needed = true;
            }
            if let Some(mut shadow) = self.building_shadows.remove(&entity) {
                shadow.queue_free();
            }
        }

        for building in buildings {
            let Some(texture) = self.building_texture(building.kind) else {
                godot_error!(
                    "GameWorld: building texture missing for {:?}",
                    building.kind
                );
                self.disable_processing();
                return redraw_needed;
            };

            let position = cell_top_left(building.footprint.origin());
            let texture_size =
                Vector2::new(texture.get_width() as f32, texture.get_height() as f32);
            let Some(scale) = building_visual_scale(building.footprint, texture_size) else {
                godot_error!(
                    "GameWorld: building texture for {:?} has invalid dimensions",
                    building.kind
                );
                self.disable_processing();
                return redraw_needed;
            };
            let footprint = footprint_rect(building.footprint);
            let z_index = world_entity_z_index(footprint.position.y + footprint.size.y);
            let shadow_position = Vector2::new(
                footprint.position.x + footprint.size.x * 0.5,
                footprint.position.y + footprint.size.y - 7.0,
            );
            let shadow_radii = Vector2::new(footprint.size.x * 0.38, 10.0);
            let modulate = building_sprite_modulate(building.state);
            if !self.building_sprites.contains_key(&building.entity) {
                let shadow = create_contact_shadow(shadow_position, shadow_radii, z_index - 1);
                self.add_2d_child(&shadow);
                let mut sprite = Sprite2D::new_alloc();
                sprite.set_texture(&texture);
                sprite.set_centered(false);
                sprite.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
                sprite.set_scale(scale);
                sprite.set_z_index(z_index);
                sprite.set_position(position);
                sprite.set_modulate(modulate);
                self.add_2d_child(&sprite);
                self.building_shadows.insert(building.entity, shadow);
                self.building_sprites.insert(building.entity, sprite);
                redraw_needed = true;
                continue;
            }

            if let Some(sprite) = self.building_sprites.get_mut(&building.entity) {
                if sprite.get_position() != position || sprite.get_modulate() != modulate {
                    redraw_needed = true;
                }
                sprite.set_texture(&texture);
                sprite.set_position(position);
                sprite.set_scale(scale);
                sprite.set_z_index(z_index);
                sprite.set_modulate(modulate);
            }
            if let Some(shadow) = self.building_shadows.get_mut(&building.entity) {
                shadow.set_position(shadow_position);
                shadow.set_z_index(z_index - 1);
            }
        }

        if let Some(selected) = self.selected_building {
            let selected_still_exists = active_entities.contains(&selected.entity);
            if !selected_still_exists {
                self.clear_building_selection();
                redraw_needed = true;
            }
        }

        redraw_needed
    }

    fn sync_npc_sprites(&mut self) {
        let npcs = self.npc_render_infos();
        if self.npc_scenes.len() != NpcAppearance::ALL.len() {
            godot_error!("GameWorld: NPC scenes not initialized");
            self.disable_processing();
            return;
        }

        let active_entities: HashSet<Entity> = npcs.iter().map(|npc| npc.entity).collect();
        let stale_entities: Vec<Entity> = self
            .npc_sprites
            .keys()
            .copied()
            .filter(|entity| !active_entities.contains(entity))
            .collect();
        for entity in stale_entities {
            if let Some(mut rendered) = self.npc_sprites.remove(&entity) {
                rendered.sprite.queue_free();
                rendered.shadow.queue_free();
            }
        }

        let selected_entity = self.selected_npc.map(|selected| selected.entity);
        let mut selected_coord = None;
        for npc in npcs {
            if selected_entity == Some(npc.entity) {
                selected_coord = Some(npc.coord);
            }

            let position = npc_top_left(npc.coord, npc.subtile_offset);
            let should_recreate = self
                .npc_sprites
                .get(&npc.entity)
                .map(|rendered| rendered.appearance != npc.appearance)
                .unwrap_or(true);

            if should_recreate {
                if let Some(mut rendered) = self.npc_sprites.remove(&npc.entity) {
                    rendered.sprite.queue_free();
                }

                let Some(npc_scene) = self.npc_scene(npc.appearance) else {
                    godot_error!(
                        "GameWorld: missing NPC scene for appearance {:?}",
                        npc.appearance
                    );
                    self.disable_processing();
                    return;
                };
                let Some(node) = npc_scene.instantiate() else {
                    godot_error!("GameWorld: failed to instantiate NPC scene");
                    self.disable_processing();
                    return;
                };

                let mut sprite = match node.try_cast::<AnimatedSprite2D>() {
                    Ok(sprite) => sprite,
                    Err(mut node) => {
                        godot_error!(
                            "GameWorld: NPC scene root is {}, expected AnimatedSprite2D",
                            node.get_class()
                        );
                        node.queue_free();
                        self.disable_processing();
                        return;
                    }
                };
                sprite.set_position(position);
                let z_index = world_entity_z_index(position.y + grid::TILE_SIZE);
                sprite.set_z_index(z_index);
                set_npc_animation(&mut sprite, npc);
                let shadow = create_contact_shadow(
                    position + Vector2::new(32.0, 57.0),
                    Vector2::new(14.0, 6.0),
                    z_index - 1,
                );
                let mut cargo_icon = Sprite2D::new_alloc();
                cargo_icon.set_centered(true);
                cargo_icon.set_position(Vector2::new(184.0, 48.0));
                cargo_icon.set_scale(Vector2::new(0.35, 0.35));
                cargo_icon.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
                cargo_icon.set_z_index(1);
                update_cargo_icon(
                    &mut cargo_icon,
                    (!npc.has_wheelbarrow).then_some(npc.carried_kind).flatten(),
                );
                sprite.add_child(&cargo_icon);
                let Some(wheelbarrow_scene) = self.wheelbarrow_scene.as_ref() else {
                    godot_error!("GameWorld: wheelbarrow scene not initialized");
                    self.disable_processing();
                    return;
                };
                let Some(wheelbarrow_node) = wheelbarrow_scene.instantiate() else {
                    godot_error!("GameWorld: failed to instantiate wheelbarrow overlay scene");
                    self.disable_processing();
                    return;
                };
                let mut wheelbarrow = match wheelbarrow_node.try_cast::<AnimatedSprite2D>() {
                    Ok(sprite) => sprite,
                    Err(mut node) => {
                        godot_error!(
                            "GameWorld: wheelbarrow scene root is {}, expected AnimatedSprite2D",
                            node.get_class()
                        );
                        node.queue_free();
                        self.disable_processing();
                        return;
                    }
                };
                let Some(frames) = self.wheelbarrow_frames.as_ref() else {
                    godot_error!("GameWorld: wheelbarrow frames not initialized");
                    self.disable_processing();
                    return;
                };
                wheelbarrow.set_sprite_frames(frames);
                wheelbarrow.set_scale(self.wheelbarrow_overlay_scale);
                set_wheelbarrow_animation(&mut wheelbarrow, npc);
                sprite.add_child(&wheelbarrow);
                self.add_2d_child(&shadow);
                self.add_2d_child(&sprite);
                self.npc_sprites.insert(
                    npc.entity,
                    RenderedNpcSprite {
                        appearance: npc.appearance,
                        sprite,
                        shadow,
                        cargo_icon,
                        carried_kind: npc.carried_kind,
                        wheelbarrow,
                        has_wheelbarrow: npc.has_wheelbarrow,
                    },
                );
                continue;
            }

            if let Some(rendered) = self.npc_sprites.get_mut(&npc.entity) {
                rendered.sprite.set_position(position);
                let z_index = world_entity_z_index(position.y + grid::TILE_SIZE);
                rendered.sprite.set_z_index(z_index);
                rendered
                    .shadow
                    .set_position(position + Vector2::new(32.0, 57.0));
                rendered.shadow.set_z_index(z_index - 1);
                set_npc_animation(&mut rendered.sprite, npc);
                if rendered.carried_kind != npc.carried_kind
                    || rendered.has_wheelbarrow != npc.has_wheelbarrow
                {
                    update_cargo_icon(
                        &mut rendered.cargo_icon,
                        (!npc.has_wheelbarrow).then_some(npc.carried_kind).flatten(),
                    );
                    rendered.carried_kind = npc.carried_kind;
                }
                set_wheelbarrow_animation(&mut rendered.wheelbarrow, npc);
                rendered.has_wheelbarrow = npc.has_wheelbarrow;
            }
        }

        if let Some(selected) = self.selected_npc {
            if let Some(coord) = selected_coord {
                if selected.coord != coord {
                    self.selected_npc = Some(SelectedNpc {
                        coord,
                        entity: selected.entity,
                    });
                    self.queue_renderer_redraw();
                }
            } else {
                self.clear_npc_selection();
            }
        }
    }

    fn build_terrain_tile_set(&self) -> Option<BuiltTileSet> {
        let mut tile_set = TileSet::new_gd();
        let mut common_metrics = None;

        for kind in TerrainKind::ALL {
            let path = terrain_asset_path(kind);
            let texture = load_texture(path, "GameWorld")?;
            let metrics = include_texture_metrics(
                &mut common_metrics,
                &texture,
                TERRAIN_VARIANT_COUNT,
                1,
                path,
            )?;
            let source_ts = build_horizontal_atlas_source(texture, metrics, TERRAIN_VARIANT_COUNT);
            let expected_source_id = terrain_source_id(kind);
            let source_id = tile_set
                .add_source_ex(&source_ts)
                .atlas_source_id_override(expected_source_id)
                .done();
            if source_id != expected_source_id {
                godot_error!(
                    "GameWorld: failed to add {} terrain tile source",
                    kind.label()
                );
                return None;
            }
        }

        finish_tile_set(tile_set, common_metrics)
    }

    fn build_resource_node_tile_set(&self) -> Option<BuiltTileSet> {
        let mut tile_set = TileSet::new_gd();
        let mut common_metrics = None;

        for kind in ResourceKind::ALL {
            let path = resource_asset_path(kind);
            let texture = load_texture(path, "GameWorld")?;
            let metrics = include_texture_metrics(&mut common_metrics, &texture, 1, 1, path)?;
            let source_ts = build_single_tile_atlas_source(texture, metrics);
            let expected_source_id = kind as i32;
            let source_id = tile_set
                .add_source_ex(&source_ts)
                .atlas_source_id_override(expected_source_id)
                .done();
            if source_id != expected_source_id {
                godot_error!(
                    "GameWorld: failed to add {} resource node tile source",
                    kind.label()
                );
                return None;
            }
        }

        finish_tile_set(tile_set, common_metrics)
    }

    fn build_road_tile_set(&self) -> Option<BuiltTileSet> {
        let mut tile_set = TileSet::new_gd();
        let mut common_metrics = None;
        for tier in RoadTier::ALL {
            let path = road_asset_path(tier);
            let texture = load_texture(path, "GameWorld")?;
            let metrics = include_texture_metrics(&mut common_metrics, &texture, 4, 4, path)?;
            let source = build_road_atlas_source(texture, metrics);
            let expected_source_id = road_source_id(tier);
            let source_id = tile_set
                .add_source_ex(&source)
                .atlas_source_id_override(expected_source_id)
                .done();
            if source_id != expected_source_id {
                godot_error!("GameWorld: failed to add {} road atlas", tier.label());
                return None;
            }
        }
        finish_tile_set(tile_set, common_metrics)
    }

    fn build_crop_tile_set(&self) -> Option<BuiltTileSet> {
        let mut tile_set = TileSet::new_gd();
        let mut common_metrics = None;

        for (state, path) in crop_tile_asset_paths() {
            let texture = load_texture(path, "GameWorld")?;
            let metrics = include_texture_metrics(&mut common_metrics, &texture, 1, 1, path)?;
            let source_ts = build_single_tile_atlas_source(texture, metrics);
            let expected_source_id = crop_source_id(state);
            let source_id = tile_set
                .add_source_ex(&source_ts)
                .atlas_source_id_override(expected_source_id)
                .done();
            if source_id != expected_source_id {
                godot_error!(
                    "GameWorld: failed to add {} crop tile source",
                    state.label()
                );
                return None;
            }
        }

        finish_tile_set(tile_set, common_metrics)
    }

    fn build_tree_plot_tile_set(&self) -> Option<BuiltTileSet> {
        let mut tile_set = TileSet::new_gd();
        let mut common_metrics = None;

        for (state, path) in tree_plot_tile_asset_paths() {
            let texture = load_texture(path, "GameWorld")?;
            let metrics = include_texture_metrics(&mut common_metrics, &texture, 1, 1, path)?;
            let source_ts = build_single_tile_atlas_source(texture, metrics);
            let expected_source_id = tree_plot_source_id(state);
            let source_id = tile_set
                .add_source_ex(&source_ts)
                .atlas_source_id_override(expected_source_id)
                .done();
            if source_id != expected_source_id {
                godot_error!(
                    "GameWorld: failed to add {} tree plot tile source",
                    state.label()
                );
                return None;
            }
        }

        finish_tile_set(tile_set, common_metrics)
    }

    fn load_building_textures(&mut self) -> bool {
        self.building_textures.clear();

        for kind in BuildingKind::ALL {
            let path = building_asset_path(kind);
            let Some(texture) = load_texture(path, "GameWorld") else {
                return false;
            };
            self.building_textures.insert(kind, texture);
        }

        true
    }

    fn building_texture(&self, kind: BuildingKind) -> Option<Gd<Texture2D>> {
        self.building_textures.get(&kind).cloned()
    }

    fn load_npc_scenes(&mut self) -> bool {
        self.npc_scenes.clear();

        for appearance in NpcAppearance::ALL {
            let path = npc_scene_path(appearance);
            let Some(scene) = load_packed_scene(path, "GameWorld") else {
                return false;
            };
            self.npc_scenes.insert(appearance, scene);
        }

        true
    }

    fn npc_scene(&self, appearance: NpcAppearance) -> Option<Gd<PackedScene>> {
        self.npc_scenes.get(&appearance).cloned()
    }

    fn load_wheelbarrow_scene(&mut self) -> bool {
        self.wheelbarrow_scene = load_packed_scene(
            WHEELBARROW_OVERLAY_SCENE_PATH,
            "GameWorld wheelbarrow overlay",
        );
        self.wheelbarrow_frames = build_wheelbarrow_frames().map(|(frames, metrics)| {
            let relative_scale = metrics.render_scale() / WorldArtMetrics::AUTHORED.render_scale();
            self.wheelbarrow_overlay_scale = Vector2::new(relative_scale, relative_scale);
            frames
        });
        self.wheelbarrow_scene.is_some() && self.wheelbarrow_frames.is_some()
    }

    fn populate_road_maps(
        &self,
        road_map: &mut Gd<TileMapLayer>,
        blueprint_map: &mut Gd<TileMapLayer>,
    ) {
        road_map.clear();
        blueprint_map.clear();
        for road in &self.dynamic_snapshot.completed_roads {
            road_map
                .set_cell_ex(Vector2i::new(road.coord.x(), road.coord.y()))
                .source_id(road_source_id(road.tier))
                .atlas_coords(road_atlas_coord(road.connectivity_mask))
                .done();
        }
        for road in &self.dynamic_snapshot.planned_roads {
            blueprint_map
                .set_cell_ex(Vector2i::new(road.coord.x(), road.coord.y()))
                .source_id(road_source_id(road.tier))
                .atlas_coords(road_atlas_coord(road.connectivity_mask))
                .done();
        }
        road_map.update_internals();
        blueprint_map.update_internals();
    }

    fn populate_resource_node_map(&mut self, resource_map: &mut Gd<TileMapLayer>) {
        resource_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let nodes = self.resource_nodes();

        for (coord, kind) in nodes {
            resource_map
                .set_cell_ex(v2(coord.x(), coord.y()))
                .source_id(kind as i32)
                .atlas_coords(v2(0, 0))
                .done();
        }
        resource_map.update_internals();
    }

    fn populate_crop_map(&mut self, crop_map: &mut Gd<TileMapLayer>) {
        crop_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let crops = self.crop_render_infos();

        for crop in crops {
            let Some(source_id) = crop_render_source_id(crop.state) else {
                continue;
            };
            crop_map
                .set_cell_ex(v2(crop.coord.x(), crop.coord.y()))
                .source_id(source_id)
                .atlas_coords(v2(0, 0))
                .done();
        }
        crop_map.update_internals();
    }

    fn populate_tree_plot_map(&mut self, tree_plot_map: &mut Gd<TileMapLayer>) {
        tree_plot_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);

        for tree_plot in self.tree_plot_render_infos() {
            let Some(source_id) = tree_plot_render_source_id(tree_plot.state) else {
                continue;
            };
            tree_plot_map
                .set_cell_ex(v2(tree_plot.coord.x(), tree_plot.coord.y()))
                .source_id(source_id)
                .atlas_coords(v2(0, 0))
                .done();
        }
        tree_plot_map.update_internals();
    }

    fn resource_nodes(&self) -> Vec<(CellCoord, ResourceKind)> {
        self.dynamic_snapshot
            .resources
            .iter()
            .map(|resource| (resource.coord, resource.kind))
            .collect()
    }

    fn building_render_infos(&self) -> Vec<BuildingRenderInfo> {
        self.dynamic_snapshot
            .buildings
            .iter()
            .map(|building| BuildingRenderInfo {
                entity: building.entity,
                kind: building.kind,
                footprint: building.footprint,
                state: match building.state {
                    snapshot::BuildingRenderState::Blueprint => BuildingRenderState::Blueprint,
                    snapshot::BuildingRenderState::Constructed => BuildingRenderState::Constructed,
                },
            })
            .collect()
    }

    fn crop_render_infos(&self) -> Vec<CropRenderInfo> {
        self.dynamic_snapshot
            .crops
            .iter()
            .map(|crop| CropRenderInfo {
                coord: crop.coord,
                state: crop.state,
            })
            .collect()
    }

    fn tree_plot_render_infos(&self) -> Vec<TreePlotRenderInfo> {
        self.dynamic_snapshot
            .tree_plots
            .iter()
            .map(|tree| TreePlotRenderInfo {
                coord: tree.coord,
                state: tree.state,
            })
            .collect()
    }

    fn npc_render_infos(&self) -> Vec<NpcRenderInfo> {
        self.dynamic_snapshot
            .npcs
            .iter()
            .map(|npc| NpcRenderInfo {
                entity: npc.entity,
                appearance: npc.appearance,
                coord: npc.position.coord,
                subtile_offset: npc.position.subtile_offset,
                velocity: npc.velocity,
                facing: npc.facing,
                is_gathering: npc.activity == snapshot::NpcActivity::Gather,
                refining_animation: match npc.activity {
                    snapshot::NpcActivity::Saw => Some(NpcRefiningAnimation::Saw),
                    snapshot::NpcActivity::Stonecut => Some(NpcRefiningAnimation::Stonecut),
                    snapshot::NpcActivity::Cook => Some(NpcRefiningAnimation::Cook),
                    _ => None,
                },
                carried_kind: npc.carried_kind,
                has_wheelbarrow: npc.has_wheelbarrow,
                wheelbarrow_kind: npc.wheelbarrow_kind,
            })
            .collect()
    }

    fn sync_selected_npc_route_overlay(&mut self) {
        let next_overlay = self.selected_npc.and_then(|selected| {
            self.with_rendered_surface_world(|world| {
                query_selected_npc_route_overlay(world, selected.entity)
            })
        });
        if self.selected_npc_route_overlay != next_overlay {
            self.selected_npc_route_overlay = next_overlay;
            self.queue_renderer_redraw();
        }
    }

    fn switch_rendered_surface(&mut self, surface: SurfaceId) {
        if self.rendered_surface == surface {
            return;
        }

        self.rendered_surface = surface;
        self.selected_cell = None;
        self.selected_npc = None;
        self.selected_npc_route_overlay = None;
        self.selected_building = None;
        self.hovered_map_entity = None;
        self.placement_mode = None;

        if !self.refresh_surface_snapshot() {
            self.disable_processing();
            return;
        }
        self.refresh_dynamic_snapshot();

        self.configure_camera_for_surface();
        if let Some(surface) = self.surface_snapshot.as_ref() {
            let width = surface.size.width_i32().unwrap_or(1);
            let height = surface.size.height_i32().unwrap_or(1);
            self.renderer_3d
                .bind_mut()
                .configure_surface(width, height, true);
        }

        let active_surface_index = surface_index_i32(self.rendered_surface);

        self.refresh_overlay_snapshot();
        match self.active_renderer_mode {
            RendererMode::TwoD => self.sync_renderer_2d_snapshots(),
            RendererMode::ThreeD => self.sync_renderer_3d_snapshots(),
        }
        self.queue_renderer_redraw();
        self.signals().tile_deselected().emit();
        self.signals().npc_deselected().emit();
        self.signals().building_deselected().emit();
        self.signals().map_entity_unhovered().emit();
        self.signals().resources_changed().emit();
        self.signals().surface_changed().emit(active_surface_index);
    }
}

#[godot_api]
impl GameWorld {
    #[signal]
    pub(crate) fn tile_selected(tile_entity_id: i64);

    #[signal]
    pub(crate) fn tile_deselected();

    #[signal]
    pub(crate) fn npc_selected(npc_entity_id: i64);

    #[signal]
    pub(crate) fn npc_deselected();

    #[signal]
    pub(crate) fn building_selected(building_entity_id: i64);

    #[signal]
    pub(crate) fn building_deselected();

    #[signal]
    pub(crate) fn resources_changed();

    #[signal]
    pub(crate) fn surface_changed(index: i32);

    #[signal]
    pub(crate) fn map_entity_hovered(kind: i64, entity_id: i64);

    #[signal]
    pub(crate) fn map_entity_unhovered();

    #[signal]
    pub(crate) fn renderer_mode_changed(mode: RendererMode);

    #[func]
    pub(crate) fn active_renderer_mode(&self) -> RendererMode {
        self.active_renderer_mode
    }

    #[func]
    pub(crate) fn renderer_mode_available(&self, mode: RendererMode) -> bool {
        match mode {
            RendererMode::TwoD => true,
            RendererMode::ThreeD => {
                self.renderer_3d.bind().availability() == Renderer3DAvailability::Ready
            }
        }
    }

    pub(crate) fn renderer_mode_unavailable_reason(
        &self,
        mode: RendererMode,
    ) -> Option<&'static str> {
        match mode {
            RendererMode::TwoD => None,
            RendererMode::ThreeD => {
                let renderer = self.renderer_3d.bind();
                match renderer.availability() {
                    Renderer3DAvailability::Preparing => Some("Preparing 3D renderer assets."),
                    Renderer3DAvailability::Ready => None,
                    Renderer3DAvailability::Failed => renderer
                        .failure_reason()
                        .or(Some("3D renderer preparation failed.")),
                }
            }
        }
    }

    #[func]
    pub(crate) fn set_renderer_mode(&mut self, mode: RendererMode) -> bool {
        if mode == self.active_renderer_mode {
            return true;
        }
        if !self.renderer_mode_available(mode) {
            return false;
        }

        if self.active_renderer_mode == RendererMode::ThreeD {
            self.end_3d_orbit_capture();
        }
        cancel_renderer_transition_drag(&mut self.placement_mode);
        let focus = match self.active_renderer_mode {
            RendererMode::TwoD => self.renderer_2d.bind().focus_tiles(),
            RendererMode::ThreeD => self.renderer_3d.bind().focus_tiles(),
        };

        match mode {
            RendererMode::TwoD => {
                if self.renderer_2d_revision != self.snapshot_revision {
                    self.sync_renderer_2d_snapshots();
                }
                self.renderer_2d.bind_mut().set_focus_tiles(focus);
                self.renderer_3d.bind_mut().set_active(false);
                self.renderer_2d.bind_mut().set_active(true);
            }
            RendererMode::ThreeD => {
                if self.renderer_3d_revision != self.snapshot_revision {
                    self.sync_renderer_3d_snapshots();
                }
                self.renderer_3d.bind_mut().set_focus_tiles(focus);
                self.renderer_2d.bind_mut().set_active(false);
                self.renderer_3d.bind_mut().set_active(true);
            }
        }

        self.active_renderer_mode = mode;
        self.refresh_overlay_snapshot();
        match mode {
            RendererMode::TwoD => self.sync_renderer_2d_snapshots(),
            RendererMode::ThreeD => self.sync_renderer_3d_snapshots(),
        }
        self.clear_hovered_map_entity();
        self.update_hovered_map_entity();
        self.signals().renderer_mode_changed().emit(mode);
        true
    }

    #[func]
    pub(crate) fn is_simulation_playing(&self) -> bool {
        self.game.is_playing()
    }

    #[func]
    pub(crate) fn toggle_simulation_playing(&mut self) {
        self.game.toggle_playing();
    }

    #[func]
    pub(crate) fn simulation_datetime_text(&self) -> GString {
        let text = self.simulation_datetime_text_string();
        GString::from(text.as_str())
    }

    #[func]
    pub(crate) fn simulation_speed_multiplier(&self) -> i32 {
        i32::try_from(self.game.simulation_speed().multiplier()).unwrap_or(i32::MAX)
    }

    #[func]
    pub(crate) fn set_simulation_speed_multiplier(&mut self, multiplier: i32) -> bool {
        let Ok(multiplier) = u32::try_from(multiplier) else {
            godot_warn!("GameWorld: ignoring negative simulation speed multiplier");
            return false;
        };
        let Some(simulation_speed) = SimulationSpeed::from_multiplier(multiplier) else {
            godot_warn!("GameWorld: ignoring unsupported simulation speed multiplier {multiplier}");
            return false;
        };

        self.game.set_simulation_speed(simulation_speed);
        true
    }

    #[func]
    pub(crate) fn surface_count(&self) -> i32 {
        i32::try_from(self.game.surface_count()).unwrap_or(i32::MAX)
    }

    #[func]
    pub(crate) fn active_surface_index(&self) -> i32 {
        surface_index_i32(self.rendered_surface)
    }

    #[func]
    pub(crate) fn set_active_surface_index(&mut self, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            godot_warn!("GameWorld: ignoring negative surface index");
            return false;
        };
        let surface = match self.game.surface_id_at(index) {
            Ok(surface) => surface,
            Err(error) => {
                godot_warn!("GameWorld: ignoring unknown surface index {index}: {error:?}");
                return false;
            }
        };

        self.switch_rendered_surface(surface);
        true
    }

    pub(crate) fn start_building_placement(&mut self, kind: BuildingKind) {
        debug_assert!(!matches!(
            kind,
            BuildingKind::Field | BuildingKind::TreePlot
        ));
        self.start_build_mode(kind);
    }

    pub(crate) fn start_road_placement(&mut self, tier: RoadTier) {
        self.start_road_placement_mode(tier);
    }

    pub(crate) fn construction_placement_status(&self) -> ConstructionPlacementStatus {
        let pointer_over_dock = self.pointer_is_over_construction_dock();
        let active_tool = self.placement_mode.as_ref().map(|mode| match mode {
            PlacementMode::Building(kind) => ConstructionTool::Building(*kind),
            PlacementMode::Plots { owner, .. } => match owner {
                PlotOwner::Farm(_) => ConstructionTool::Field,
                PlotOwner::ForesterLodge(_) => ConstructionTool::TreePlot,
            },
            PlacementMode::Roads { tier, .. } => ConstructionTool::Road(*tier),
        });

        let building_feedback = match self.placement_mode.as_ref() {
            Some(PlacementMode::Building(_)) if pointer_over_dock => {
                Some(BuildingPlacementFeedback::MoveCursorOverMap)
            }
            Some(PlacementMode::Building(kind)) => match self.placement_origin_under_mouse() {
                None => Some(BuildingPlacementFeedback::MoveCursorOverMap),
                Some(origin) => Some(
                    match self.game.validate_building_blueprint_placement(
                        self.rendered_surface,
                        *kind,
                        origin,
                    ) {
                        Ok(_) => BuildingPlacementFeedback::Valid,
                        Err(error) => BuildingPlacementFeedback::Invalid(error),
                    },
                ),
            },
            _ => None,
        };

        let road = match self.placement_mode.as_ref() {
            Some(PlacementMode::Roads { tier, .. }) if pointer_over_dock => {
                Some(RoadPlacementStatus {
                    active_tier: Some(*tier),
                    cell_count: 0,
                    invalid_cell_count: 0,
                    aggregate_cost: ResourceAmounts::zero(),
                    errors: Vec::new(),
                })
            }
            Some(PlacementMode::Roads { .. }) => Some(self.road_placement_status()),
            _ => None,
        };

        ConstructionPlacementStatus {
            active_tool,
            building_feedback,
            road,
        }
    }

    pub(crate) fn road_placement_status(&self) -> RoadPlacementStatus {
        let active_tier = match self.placement_mode.as_ref() {
            Some(PlacementMode::Roads { tier, .. }) => Some(*tier),
            _ => None,
        };
        let validation = self.road_validation();
        RoadPlacementStatus {
            active_tier,
            cell_count: validation.as_ref().map_or(0, |result| result.cells.len()),
            invalid_cell_count: validation.as_ref().map_or(0, |result| {
                result
                    .cells
                    .iter()
                    .filter(|cell| !cell.errors.is_empty())
                    .count()
            }),
            aggregate_cost: validation
                .as_ref()
                .map_or(ResourceAmounts::zero(), |result| result.aggregate_cost),
            errors: validation
                .into_iter()
                .flat_map(|result| result.cells)
                .flat_map(|cell| {
                    cell.errors
                        .into_iter()
                        .map(move |error| (cell.coord, error))
                })
                .collect(),
        }
    }

    pub(crate) fn cancel_construction_placement(&mut self) {
        self.cancel_placement_mode();
    }

    #[func]
    pub(crate) fn start_field_placement_for_selected_farm(&mut self) -> bool {
        let Some(selected) = self.selected_building else {
            godot_warn!("GameWorld: cannot start field placement without a selected Farm");
            return false;
        };
        let is_farm = self.with_rendered_surface_world(|world| {
            world
                .get::<Building>(selected.entity)
                .is_some_and(|building| building.kind == BuildingKind::Farm)
                || world
                    .get::<BuildingBlueprint>(selected.entity)
                    .is_some_and(|blueprint| blueprint.kind == BuildingKind::Farm)
        });
        if !is_farm {
            godot_warn!("GameWorld: selected building is not a Farm");
            return false;
        }

        self.start_plot_placement_mode(PlotOwner::Farm(selected.entity));
        true
    }

    #[func]
    pub(crate) fn start_tree_plot_placement_for_selected_lodge(&mut self) -> bool {
        let Some(selected) = self.selected_building else {
            godot_warn!(
                "GameWorld: cannot start tree plot placement without a selected Forester's Lodge"
            );
            return false;
        };
        let is_lodge = self.with_rendered_surface_world(|world| {
            world
                .get::<Building>(selected.entity)
                .is_some_and(|building| building.kind == BuildingKind::ForesterLodge)
                || world
                    .get::<BuildingBlueprint>(selected.entity)
                    .is_some_and(|blueprint| blueprint.kind == BuildingKind::ForesterLodge)
        });
        if !is_lodge {
            godot_warn!("GameWorld: selected building is not a Forester's Lodge");
            return false;
        }

        self.start_plot_placement_mode(PlotOwner::ForesterLodge(selected.entity));
        true
    }

    pub(crate) fn dismiss_building_context_from_panel(&mut self) {
        if self.selected_building.take().is_some() {
            self.queue_renderer_redraw();
        }
    }

    pub(crate) fn rename_building(
        &mut self,
        building_entity_id: BridgeEntityId,
        requested_name: &str,
    ) -> Result<(), BuildingCommandError> {
        let target = BuildingTarget::new(self.rendered_surface, building_entity_id.entity());
        self.game
            .rename_building(self.rendered_surface, target, requested_name)
    }

    pub(crate) fn set_building_active(
        &mut self,
        building_entity_id: BridgeEntityId,
        active: bool,
    ) -> Result<(), BuildingCommandError> {
        let target = BuildingTarget::new(self.rendered_surface, building_entity_id.entity());
        self.game
            .set_building_active(self.rendered_surface, target, active)
    }

    pub(crate) fn set_storage_resource_allowed(
        &mut self,
        building_entity_id: BridgeEntityId,
        kind: ResourceKind,
        allowed: bool,
    ) -> Result<(), BuildingCommandError> {
        let target = BuildingTarget::new(self.rendered_surface, building_entity_id.entity());
        self.game
            .set_storage_resource_allowed(self.rendered_surface, target, kind, allowed)
    }

    pub(crate) fn set_storage_pulls_from_refineries(
        &mut self,
        building_entity_id: BridgeEntityId,
        kind: ResourceKind,
        enabled: bool,
    ) -> Result<(), BuildingCommandError> {
        let target = BuildingTarget::new(self.rendered_surface, building_entity_id.entity());
        self.game
            .set_storage_pulls_from_refineries(self.rendered_surface, target, kind, enabled)
    }

    pub(crate) fn set_refinery_pulls_from_storage(
        &mut self,
        building_entity_id: BridgeEntityId,
        kind: ResourceKind,
        enabled: bool,
    ) -> Result<(), BuildingCommandError> {
        let target = BuildingTarget::new(self.rendered_surface, building_entity_id.entity());
        self.game
            .set_refinery_pulls_from_storage(self.rendered_surface, target, kind, enabled)
    }

    #[func]
    pub(crate) fn cancel_building_blueprint_placement(&mut self) {
        self.cancel_placement_mode();
    }
}

fn surface_index_i32(surface: SurfaceId) -> i32 {
    i32::try_from(surface.index()).unwrap_or(i32::MAX)
}

fn click_selection_targets_at(world: &World, coord: CellCoord) -> ClickSelectionTargets {
    ClickSelectionTargets {
        tile: selected_cell_at(world, coord),
        npc: selected_npc_at(world, coord),
        building: selected_building_at(world, coord),
    }
}

fn selected_cell_at(world: &World, coord: CellCoord) -> Option<SelectedCell> {
    let index = world.get_resource::<TileIndex>()?;
    let entity = index.get(coord)?;
    world.get::<Tile>(entity)?;
    Some(SelectedCell { coord, entity })
}

fn selected_npc_at(world: &World, coord: CellCoord) -> Option<SelectedNpc> {
    let mut query = world.try_query::<(Entity, &NpcPosition, &Npc)>()?;
    query
        .iter(world)
        .filter_map(|(entity, position, _)| {
            (position.coord == coord).then_some(SelectedNpc { coord, entity })
        })
        .min_by_key(|selected| selected.entity.to_bits())
}

fn selected_npc_for_entity(world: &World, entity: Entity) -> Option<SelectedNpc> {
    world.get::<Npc>(entity)?;
    let position = world.get::<NpcPosition>(entity)?;
    Some(SelectedNpc {
        coord: position.coord,
        entity,
    })
}

fn selected_building_at(world: &World, coord: CellCoord) -> Option<SelectedBuilding> {
    let blueprint = world
        .try_query::<(Entity, &BuildingBlueprint)>()
        .and_then(|mut query| {
            query.iter(world).find_map(|(entity, blueprint)| {
                blueprint
                    .footprint
                    .contains(coord)
                    .then_some(SelectedBuilding {
                        footprint: blueprint.footprint,
                        entity,
                    })
            })
        });
    if blueprint.is_some() {
        return blueprint;
    }

    world
        .try_query::<(Entity, &Building)>()
        .and_then(|mut query| {
            query.iter(world).find_map(|(entity, building)| {
                building
                    .footprint
                    .contains(coord)
                    .then_some(SelectedBuilding {
                        footprint: building.footprint,
                        entity,
                    })
            })
        })
}

fn selected_building_for_entity(world: &World, entity: Entity) -> Option<SelectedBuilding> {
    if let Some(blueprint) = world.get::<BuildingBlueprint>(entity) {
        return Some(SelectedBuilding {
            footprint: blueprint.footprint,
            entity,
        });
    }

    world
        .get::<Building>(entity)
        .map(|building| SelectedBuilding {
            footprint: building.footprint,
            entity,
        })
}

fn apply_click_selection_targets(
    selected_cell: &mut Option<SelectedCell>,
    selected_npc: &mut Option<SelectedNpc>,
    selected_building: &mut Option<SelectedBuilding>,
    targets: ClickSelectionTargets,
) -> Vec<SelectionEvent> {
    let mut events = Vec::new();

    if targets.tile.is_none() && selected_cell.take().is_some() {
        events.push(SelectionEvent::TileDeselected);
    }
    if targets.npc.is_none() && selected_npc.take().is_some() {
        events.push(SelectionEvent::NpcDeselected);
    }
    if targets.building.is_none() && selected_building.take().is_some() {
        events.push(SelectionEvent::BuildingDeselected);
    }

    if let Some(tile) = targets.tile {
        *selected_cell = Some(tile);
        events.push(SelectionEvent::TileSelected(tile.entity));
    }
    if let Some(npc) = targets.npc {
        *selected_npc = Some(npc);
        events.push(SelectionEvent::NpcSelected(npc.entity));
    }
    if let Some(building) = targets.building {
        *selected_building = Some(building);
        events.push(SelectionEvent::BuildingSelected(building.entity));
    }

    events
}

fn map_entity_target_at(world: &World, coord: CellCoord) -> Option<MapEntityTarget> {
    if let Some(entity) = building_entity_at(world, coord) {
        return Some(MapEntityTarget {
            kind: MapEntityKind::Building,
            entity,
        });
    }

    if let Some(entity) = road_blueprint_entity_at(world, coord) {
        return Some(MapEntityTarget {
            kind: MapEntityKind::RoadBlueprint,
            entity,
        });
    }

    if let Some(entity) = npc_entity_at(world, coord) {
        return Some(MapEntityTarget {
            kind: MapEntityKind::Npc,
            entity,
        });
    }

    resource_node_entity_at(world, coord).map(|entity| MapEntityTarget {
        kind: MapEntityKind::ResourceNode,
        entity,
    })
}

fn map_entity_target_from_proxy(world: &World, hit: ProxyHit3D) -> Option<MapEntityTarget> {
    let kind = match hit.kind {
        ProxyKind3D::Building
            if world.get::<Building>(hit.entity).is_some()
                || world.get::<BuildingBlueprint>(hit.entity).is_some() =>
        {
            MapEntityKind::Building
        }
        ProxyKind3D::Npc if world.get::<Npc>(hit.entity).is_some() => MapEntityKind::Npc,
        ProxyKind3D::Resource if world.get::<ResourceNode>(hit.entity).is_some() => {
            MapEntityKind::ResourceNode
        }
        ProxyKind3D::RoadBlueprint if world.get::<RoadBlueprint>(hit.entity).is_some() => {
            MapEntityKind::RoadBlueprint
        }
        _ => return None,
    };

    Some(MapEntityTarget {
        kind,
        entity: hit.entity,
    })
}

fn building_entity_at(world: &World, coord: CellCoord) -> Option<Entity> {
    selected_building_at(world, coord).map(|selected| selected.entity)
}

fn road_blueprint_entity_at(world: &World, coord: CellCoord) -> Option<Entity> {
    if let Some(entity) = world
        .get_resource::<RoadMap>()
        .and_then(|roads| roads.entity_at(coord))
        .filter(|entity| world.get::<RoadBlueprint>(*entity).is_some())
    {
        return Some(entity);
    }

    let mut query = world.try_query::<(Entity, &RoadBlueprint)>()?;
    query
        .iter(world)
        .find_map(|(entity, blueprint)| (blueprint.coord == coord).then_some(entity))
}

fn npc_entity_at(world: &World, coord: CellCoord) -> Option<Entity> {
    let mut query = world.try_query::<(Entity, &NpcPosition, &Npc)>()?;
    query
        .iter(world)
        .find_map(|(entity, position, _)| (position.coord == coord).then_some(entity))
}

fn resource_node_entity_at(world: &World, coord: CellCoord) -> Option<Entity> {
    let mut query = world.try_query::<(Entity, &TilePosition, &ResourceNode, &Tile)>()?;
    query
        .iter(world)
        .find_map(|(entity, position, _, _)| (position.coord == coord).then_some(entity))
}

#[cfg(test)]
fn query_building_render_infos(world: &World) -> Vec<BuildingRenderInfo> {
    let mut buildings = world
        .try_query::<(Entity, &BuildingBlueprint)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, blueprint)| BuildingRenderInfo {
                    entity,
                    kind: blueprint.kind,
                    footprint: blueprint.footprint,
                    state: BuildingRenderState::Blueprint,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(mut query) = world.try_query::<(Entity, &Building)>() {
        buildings.extend(
            query
                .iter(world)
                .map(|(entity, building)| BuildingRenderInfo {
                    entity,
                    kind: building.kind,
                    footprint: building.footprint,
                    state: BuildingRenderState::Constructed,
                }),
        );
    }

    buildings.sort_by_key(|building| building.entity.to_bits());
    buildings
}

#[cfg(test)]
fn query_crop_render_infos(world: &World) -> Vec<CropRenderInfo> {
    world
        .try_query::<(Entity, &Building, &FieldCrop)>()
        .map(|mut query| {
            let mut crops = query
                .iter(world)
                .filter_map(|(entity, building, _)| {
                    if building.kind != BuildingKind::Field {
                        return None;
                    }
                    let state = field_crop_state(world, entity)?;
                    Some(CropRenderInfo {
                        coord: building.footprint.origin(),
                        state,
                    })
                })
                .collect::<Vec<_>>();
            crops.sort_by_key(|crop| (crop.coord.y(), crop.coord.x()));
            crops
        })
        .unwrap_or_default()
}

#[cfg(test)]
fn query_tree_plot_render_infos(world: &World) -> Vec<TreePlotRenderInfo> {
    world
        .try_query::<(Entity, &Building, &TreePlotGrowth)>()
        .map(|mut query| {
            let mut tree_plots = query
                .iter(world)
                .filter_map(|(entity, building, _)| {
                    if building.kind != BuildingKind::TreePlot {
                        return None;
                    }
                    Some(TreePlotRenderInfo {
                        coord: building.footprint.origin(),
                        state: tree_plot_state(world, entity)?,
                    })
                })
                .collect::<Vec<_>>();
            tree_plots.sort_by_key(|tree_plot| (tree_plot.coord.y(), tree_plot.coord.x()));
            tree_plots
        })
        .unwrap_or_default()
}

#[cfg(test)]
fn query_npc_render_infos(world: &World) -> Vec<NpcRenderInfo> {
    world
        .try_query::<(Entity, &NpcPosition, Option<&NpcAppearance>, &Npc)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, position, appearance, _)| NpcRenderInfo {
                    entity,
                    appearance: appearance.copied().unwrap_or_default(),
                    coord: position.coord,
                    subtile_offset: position.subtile_offset,
                    velocity: world.get::<Velocity>(entity).copied().unwrap_or_default(),
                    facing: world
                        .get::<MovementFacing>(entity)
                        .copied()
                        .unwrap_or_default(),
                    is_gathering: world.get::<AiGatherResource>(entity).is_some()
                        || world.get::<AiSeedField>(entity).is_some()
                        || world.get::<AiHarvestField>(entity).is_some()
                        || world.get::<AiSeedTreePlot>(entity).is_some()
                        || world.get::<AiCutTreePlot>(entity).is_some(),
                    refining_animation: world
                        .get::<AiRefineResource>(entity)
                        .filter(|work| work.is_actively_processing())
                        .map(|work| NpcRefiningAnimation::from_recipe(work.recipe())),
                    carried_kind: world
                        .get::<CarriedResource>(entity)
                        .and_then(|cargo| cargo.stack())
                        .map(|stack| stack.kind()),
                    has_wheelbarrow: world.get::<Wheelbarrow>(entity).is_some(),
                    wheelbarrow_kind: world
                        .get::<Wheelbarrow>(entity)
                        .and_then(|wheelbarrow| wheelbarrow.stack())
                        .map(|stack| stack.kind()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn query_selected_npc_route_overlay(
    world: &World,
    entity: Entity,
) -> Option<SelectedNpcRouteOverlay> {
    let position = *world.get::<NpcPosition>(entity)?;
    let route = world.get::<NpcRoute>(entity)?;
    if route.is_blocked() {
        return Some(SelectedNpcRouteOverlay::Blocked { position });
    }

    Some(SelectedNpcRouteOverlay::Route {
        position,
        waypoints: route.waypoints().collect(),
        destination: route.destination()?,
    })
}

fn cell_top_left(coord: CellCoord) -> Vector2 {
    Vector2::new(
        coord.x() as f32 * grid::TILE_SIZE,
        coord.y() as f32 * grid::TILE_SIZE,
    )
}

#[cfg(test)]
fn cell_center(coord: CellCoord) -> Vector2 {
    cell_top_left(coord) + Vector2::new(grid::TILE_SIZE / 2.0, grid::TILE_SIZE / 2.0)
}

fn npc_top_left(coord: CellCoord, subtile_offset: SubtileOffset) -> Vector2 {
    cell_top_left(coord)
        + Vector2::new(
            subtile_units_to_pixels(subtile_offset.x_units),
            subtile_units_to_pixels(subtile_offset.y_units),
        )
}

#[cfg(test)]
fn npc_center(position: NpcPosition) -> Vector2 {
    npc_top_left(position.coord, position.subtile_offset)
        + Vector2::new(grid::TILE_SIZE / 2.0, grid::TILE_SIZE / 2.0)
}

#[cfg(test)]
fn npc_route_points(position: NpcPosition, waypoints: &[CellCoord]) -> Vec<Vector2> {
    let mut points = Vec::with_capacity(waypoints.len() + 1);
    points.push(npc_center(position));
    for waypoint in waypoints {
        let point = cell_center(*waypoint);
        if points.last().copied() != Some(point) {
            points.push(point);
        }
    }
    points
}

#[cfg(test)]
fn route_chevron(from: Vector2, to: Vector2) -> Option<[Vector2; 3]> {
    let delta = to - from;
    let length = delta.length();
    if length <= f32::EPSILON {
        return None;
    }

    let direction = delta / length;
    let perpendicular = Vector2::new(-direction.y, direction.x);
    let midpoint = (from + to) * 0.5;
    let tip = midpoint + direction * 6.0;
    let tail = midpoint - direction * 6.0;
    Some([tail + perpendicular * 5.0, tip, tail - perpendicular * 5.0])
}

fn subtile_units_to_pixels(units: i32) -> f32 {
    units as f32 * grid::TILE_SIZE / SUBTILE_UNITS_PER_TILE as f32
}

fn set_npc_animation(sprite: &mut Gd<AnimatedSprite2D>, npc: NpcRenderInfo) {
    let animation = StringName::from(npc_animation_name(npc));
    if sprite.get_animation() != animation {
        sprite.set_animation(&animation);
    }
    if !sprite.is_playing() {
        sprite.play();
    }
}

fn update_cargo_icon(icon: &mut Gd<Sprite2D>, kind: Option<ResourceKind>) {
    let Some(kind) = kind else {
        icon.set_texture(Gd::null_arg());
        icon.hide();
        return;
    };
    if let Some(texture) = load_texture(resource_asset_path(kind), "GameWorld cargo overlay") {
        if let Some(metrics) = WorldArtMetrics::from_texture(&texture) {
            let relative_scale =
                0.35 * metrics.render_scale() / WorldArtMetrics::AUTHORED.render_scale();
            icon.set_scale(Vector2::new(relative_scale, relative_scale));
        }
        icon.set_texture(&texture);
        icon.show();
    } else {
        icon.hide();
    }
}

fn build_wheelbarrow_frames() -> Option<(Gd<SpriteFrames>, WorldArtMetrics)> {
    const DIRECTIONS: [&str; 8] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
    let sheets = std::iter::once(("empty", WHEELBARROW_EMPTY_PATH)).chain(
        ResourceKind::ALL
            .into_iter()
            .map(|kind| (resource_animation_slug(kind), wheelbarrow_asset_path(kind))),
    );
    let mut frames = SpriteFrames::new_gd();
    frames.clear_all();
    let mut common_metrics = None;

    for (load, path) in sheets {
        let texture = load_texture(path, "GameWorld wheelbarrow frames")?;
        let metrics = include_texture_metrics(&mut common_metrics, &texture, 4, 8, path)?;
        for (row, direction) in DIRECTIONS.into_iter().enumerate() {
            let animation_name = format!("{load}_{direction}");
            let animation = StringName::from(&animation_name);
            frames.add_animation(&animation);
            frames.set_animation_speed(&animation, 8.0);
            frames.set_animation_loop(&animation, true);
            for column in 0..4 {
                let mut atlas = AtlasTexture::new_gd();
                atlas.set_atlas(&texture);
                atlas.set_region(metrics.atlas_region(column, row as i32));
                let frame: Gd<Texture2D> = atlas.upcast();
                frames.add_frame(&animation, &frame);
            }
        }
    }

    Some((frames, common_metrics?))
}

fn set_wheelbarrow_animation(sprite: &mut Gd<AnimatedSprite2D>, npc: NpcRenderInfo) {
    if !npc.has_wheelbarrow {
        sprite.hide();
        return;
    }

    sprite.show();
    let (position, z_index) = wheelbarrow_transform(npc.facing);
    sprite.set_position(position);
    sprite.set_z_index(z_index);
    let animation_name = wheelbarrow_animation_name(npc.facing, npc.wheelbarrow_kind);
    let animation = StringName::from(&animation_name);
    if sprite.get_animation() != animation {
        sprite.set_animation(&animation);
    }
    if npc.velocity.is_zero() {
        sprite.stop();
        sprite.set_frame(0);
    } else if !sprite.is_playing() {
        sprite.play();
    }
}

fn wheelbarrow_transform(facing: MovementFacing) -> (Vector2, i32) {
    // The overlay is a child of the authored 0.25-scale NPC sprite, so its
    // local offsets use source-pixel coordinates.
    const CARDINAL_OFFSET: f32 = 96.0;
    const DIAGONAL_OFFSET: f32 = 72.0;

    let position = match facing {
        MovementFacing::North => Vector2::new(0.0, -CARDINAL_OFFSET),
        MovementFacing::NorthEast => Vector2::new(DIAGONAL_OFFSET, -DIAGONAL_OFFSET),
        MovementFacing::East => Vector2::new(CARDINAL_OFFSET, 0.0),
        MovementFacing::SouthEast => Vector2::new(DIAGONAL_OFFSET, DIAGONAL_OFFSET),
        MovementFacing::South => Vector2::new(0.0, CARDINAL_OFFSET),
        MovementFacing::SouthWest => Vector2::new(-DIAGONAL_OFFSET, DIAGONAL_OFFSET),
        MovementFacing::West => Vector2::new(-CARDINAL_OFFSET, 0.0),
        MovementFacing::NorthWest => Vector2::new(-DIAGONAL_OFFSET, -DIAGONAL_OFFSET),
    };
    let z_index = match facing {
        MovementFacing::North | MovementFacing::NorthEast | MovementFacing::NorthWest => -1,
        MovementFacing::East
        | MovementFacing::SouthEast
        | MovementFacing::South
        | MovementFacing::SouthWest
        | MovementFacing::West => 1,
    };

    (position, z_index)
}

fn wheelbarrow_animation_name(facing: MovementFacing, cargo: Option<ResourceKind>) -> String {
    let load = cargo.map_or("empty", resource_animation_slug);
    let direction = match facing {
        MovementFacing::North => "n",
        MovementFacing::NorthEast => "ne",
        MovementFacing::East => "e",
        MovementFacing::SouthEast => "se",
        MovementFacing::South => "s",
        MovementFacing::SouthWest => "sw",
        MovementFacing::West => "w",
        MovementFacing::NorthWest => "nw",
    };
    format!("{load}_{direction}")
}

const fn resource_animation_slug(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "wood",
        ResourceKind::Stone => "stone",
        ResourceKind::Food => "food",
        ResourceKind::Gold => "gold",
        ResourceKind::Crops => "crops",
        ResourceKind::WildBerries => "wild_berries",
        ResourceKind::Planks => "planks",
        ResourceKind::StoneBlocks => "stone_blocks",
    }
}

const fn wheelbarrow_asset_path(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "res://assets/visual/world/vehicles/wheelbarrow_wood_sheet.png",
        ResourceKind::Stone => "res://assets/visual/world/vehicles/wheelbarrow_stone_sheet.png",
        ResourceKind::Food => "res://assets/visual/world/vehicles/wheelbarrow_food_sheet.png",
        ResourceKind::Gold => "res://assets/visual/world/vehicles/wheelbarrow_gold_sheet.png",
        ResourceKind::Crops => "res://assets/visual/world/vehicles/wheelbarrow_crops_sheet.png",
        ResourceKind::WildBerries => {
            "res://assets/visual/world/vehicles/wheelbarrow_wild_berries_sheet.png"
        }
        ResourceKind::Planks => "res://assets/visual/world/vehicles/wheelbarrow_planks_sheet.png",
        ResourceKind::StoneBlocks => {
            "res://assets/visual/world/vehicles/wheelbarrow_stone_blocks_sheet.png"
        }
    }
}

fn npc_animation_name(npc: NpcRenderInfo) -> &'static str {
    if let Some(refining_animation) = npc.refining_animation {
        return refining_animation.animation_name();
    }

    if npc.is_gathering {
        return "gather";
    }

    if npc.velocity.is_zero() {
        return "idle";
    }

    match npc.facing {
        MovementFacing::North => "walk_n",
        MovementFacing::NorthEast => "walk_ne",
        MovementFacing::East => "walk_e",
        MovementFacing::SouthEast => "walk_se",
        MovementFacing::South => "walk_s",
        MovementFacing::SouthWest => "walk_sw",
        MovementFacing::West => "walk_w",
        MovementFacing::NorthWest => "walk_nw",
    }
}

fn footprint_rect(footprint: BuildingFootprint) -> Rect2 {
    Rect2::new(
        cell_top_left(footprint.origin()),
        Vector2::new(
            footprint.width() as f32 * grid::TILE_SIZE,
            footprint.height() as f32 * grid::TILE_SIZE,
        ),
    )
}

fn building_visual_scale(footprint: BuildingFootprint, texture_size: Vector2) -> Option<Vector2> {
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return None;
    }
    let logical_size = footprint_rect(footprint).size;
    Some(Vector2::new(
        logical_size.x / texture_size.x,
        logical_size.y / texture_size.y,
    ))
}

fn world_entity_z_index(feet_y: f32) -> i32 {
    WORLD_ENTITY_Z_BASE.saturating_add(feet_y.round().clamp(0.0, 4090.0) as i32)
}

fn create_contact_shadow(position: Vector2, radii: Vector2, z_index: i32) -> Gd<Polygon2D> {
    let mut shadow = Polygon2D::new_alloc();
    shadow.set_polygon(&ellipse_polygon(radii, 20));
    shadow.set_color(Color::from_rgba(0.035, 0.055, 0.07, 0.24));
    shadow.set_position(position);
    shadow.set_z_index(z_index);
    shadow
}

fn ellipse_polygon(radii: Vector2, segments: usize) -> PackedVector2Array {
    PackedVector2Array::from(ellipse_points(radii, segments))
}

fn ellipse_points(radii: Vector2, segments: usize) -> Vec<Vector2> {
    if segments < 3 || radii.x <= 0.0 || radii.y <= 0.0 {
        return Vec::new();
    }
    (0..segments)
        .map(|index| {
            let angle = std::f32::consts::TAU * index as f32 / segments as f32;
            Vector2::new(angle.cos() * radii.x, angle.sin() * radii.y)
        })
        .collect()
}

fn building_sprite_modulate(state: BuildingRenderState) -> Color {
    match state {
        BuildingRenderState::Blueprint => {
            let mut color = Color::from_rgb(0.55, 0.9, 1.0);
            color.a = 0.62;
            color
        }
        BuildingRenderState::Constructed => Color::from_rgb(1.0, 1.0, 1.0),
    }
}

fn terrain_source_id(kind: TerrainKind) -> i32 {
    kind as i32
}

fn terrain_variant(surface: SurfaceId, coord: CellCoord, kind: TerrainKind) -> i32 {
    let mut hash = surface.index() as u64 ^ 0x9e37_79b9_7f4a_7c15;
    hash ^= (coord.x() as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= (coord.y() as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^= (kind as u64).wrapping_mul(0xd6e8_feb8_6659_fd93);
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^= hash >> 31;
    (hash % TERRAIN_VARIANT_COUNT as u64) as i32
}

fn road_source_id(tier: RoadTier) -> i32 {
    match tier {
        RoadTier::DirtPath => 0,
        RoadTier::Cobblestone => 1,
        RoadTier::Flagstone => 2,
    }
}

fn road_atlas_coord(mask: u8) -> Vector2i {
    Vector2i::new(i32::from(mask % 4), i32::from(mask / 4))
}

#[cfg(test)]
fn road_connectivity_mask(coord: CellCoord, cells: &HashSet<CellCoord>) -> u8 {
    let mut mask = 0;
    if coord
        .y()
        .checked_sub(1)
        .is_some_and(|y| cells.contains(&CellCoord::new(coord.x(), y)))
    {
        mask |= 1;
    }
    if coord
        .x()
        .checked_add(1)
        .is_some_and(|x| cells.contains(&CellCoord::new(x, coord.y())))
    {
        mask |= 2;
    }
    if coord
        .y()
        .checked_add(1)
        .is_some_and(|y| cells.contains(&CellCoord::new(coord.x(), y)))
    {
        mask |= 4;
    }
    if coord
        .x()
        .checked_sub(1)
        .is_some_and(|x| cells.contains(&CellCoord::new(x, coord.y())))
    {
        mask |= 8;
    }
    mask
}

fn crop_tile_asset_paths() -> [(FieldCropState, &'static str); 4] {
    [
        (FieldCropState::Seedable, CROP_SEEDABLE_PATH),
        (FieldCropState::GrowingStep1, CROP_GROWING_STEP1_PATH),
        (FieldCropState::GrowingStep2, CROP_GROWING_STEP2_PATH),
        (FieldCropState::Grown, CROP_GROWN_PATH),
    ]
}

fn crop_source_id(state: FieldCropState) -> i32 {
    match state {
        FieldCropState::Seedable | FieldCropState::Seeding => 0,
        FieldCropState::GrowingStep1 => 1,
        FieldCropState::GrowingStep2 => 2,
        FieldCropState::Grown => 3,
        FieldCropState::Inactive => -1,
    }
}

fn crop_render_source_id(state: FieldCropState) -> Option<i32> {
    match state {
        FieldCropState::Inactive => None,
        _ => Some(crop_source_id(state)),
    }
}

fn tree_plot_tile_asset_paths() -> [(TreePlotState, &'static str); 3] {
    [
        (TreePlotState::Sapling, TREE_PLOT_SAPLING_PATH),
        (TreePlotState::Young, TREE_PLOT_YOUNG_PATH),
        (TreePlotState::Mature, TREE_PLOT_MATURE_PATH),
    ]
}

fn tree_plot_source_id(state: TreePlotState) -> i32 {
    match state {
        TreePlotState::Sapling => 0,
        TreePlotState::Young => 1,
        TreePlotState::Mature => 2,
        TreePlotState::Inactive | TreePlotState::Seedable | TreePlotState::Seeding => -1,
    }
}

fn tree_plot_render_source_id(state: TreePlotState) -> Option<i32> {
    match state {
        TreePlotState::Sapling | TreePlotState::Young | TreePlotState::Mature => {
            Some(tree_plot_source_id(state))
        }
        TreePlotState::Inactive | TreePlotState::Seedable | TreePlotState::Seeding => None,
    }
}

fn append_plot_drag_cell(drag_cells: &mut Vec<CellCoord>, coord: Option<CellCoord>) {
    let Some(coord) = coord else {
        return;
    };
    if drag_cells.last().copied() != Some(coord) {
        drag_cells.push(coord);
    }
}

fn cancel_renderer_transition_drag(placement_mode: &mut Option<PlacementMode>) -> bool {
    let drag_cells = match placement_mode {
        Some(PlacementMode::Plots { drag_cells, .. })
        | Some(PlacementMode::Roads { drag_cells, .. }) => drag_cells,
        Some(PlacementMode::Building(_)) | None => return false,
    };
    let had_drag = !drag_cells.is_empty();
    drag_cells.clear();
    had_drag
}

const fn renderer_mode_after_availability_check(
    active: RendererMode,
    availability: Renderer3DAvailability,
) -> RendererMode {
    match (active, availability) {
        (
            RendererMode::ThreeD,
            Renderer3DAvailability::Preparing | Renderer3DAvailability::Failed,
        ) => RendererMode::TwoD,
        _ => active,
    }
}

fn append_road_drag_cell(drag_cells: &mut Vec<CellCoord>, coord: Option<CellCoord>) {
    let Some(coord) = coord else {
        return;
    };
    let Some(previous) = drag_cells.last().copied() else {
        drag_cells.push(coord);
        return;
    };
    let mut x = previous.x();
    while x != coord.x() {
        x += (coord.x() - x).signum();
        let next = CellCoord::new(x, previous.y());
        if !drag_cells.contains(&next) {
            drag_cells.push(next);
        }
    }
    let mut y = previous.y();
    while y != coord.y() {
        y += (coord.y() - y).signum();
        let next = CellCoord::new(coord.x(), y);
        if !drag_cells.contains(&next) {
            drag_cells.push(next);
        }
    }
}

fn include_texture_metrics(
    common: &mut Option<WorldArtMetrics>,
    texture: &Gd<Texture2D>,
    columns: i32,
    rows: i32,
    path: &str,
) -> Option<WorldArtMetrics> {
    let sheet_size = Vector2i::new(texture.get_width(), texture.get_height());
    let Some(metrics) = WorldArtMetrics::from_sheet_size(sheet_size, columns, rows) else {
        godot_error!(
            "GameWorld: visual asset {path} has invalid {columns}x{rows} sheet dimensions {}x{}",
            sheet_size.x,
            sheet_size.y
        );
        return None;
    };

    if let Some(expected) = *common {
        if expected != metrics {
            godot_error!(
                "GameWorld: visual asset {path} uses {}px frames, expected {}px frames",
                metrics.source_frame_size(),
                expected.source_frame_size()
            );
            return None;
        }
    } else {
        *common = Some(metrics);
    }
    Some(metrics)
}

fn finish_tile_set(
    mut tile_set: Gd<TileSet>,
    metrics: Option<WorldArtMetrics>,
) -> Option<BuiltTileSet> {
    let metrics = metrics?;
    let frame_size = metrics.source_frame_size();
    tile_set.set_tile_size(Vector2i::new(frame_size, frame_size));
    Some(BuiltTileSet { tile_set, metrics })
}

fn build_single_tile_atlas_source(
    texture: Gd<Texture2D>,
    metrics: WorldArtMetrics,
) -> Gd<TileSetSource> {
    build_horizontal_atlas_source(texture, metrics, 1)
}

fn build_horizontal_atlas_source(
    texture: Gd<Texture2D>,
    metrics: WorldArtMetrics,
    columns: i32,
) -> Gd<TileSetSource> {
    let v2 = |x: i32, y: i32| Vector2i::new(x, y);
    let mut source = TileSetAtlasSource::new_gd();
    source.set_texture(&texture);
    let frame_size = metrics.source_frame_size();
    source.set_texture_region_size(v2(frame_size, frame_size));
    for column in 0..columns {
        source.create_tile_ex(v2(column, 0)).done();
    }
    source.upcast::<TileSetSource>()
}

fn build_road_atlas_source(texture: Gd<Texture2D>, metrics: WorldArtMetrics) -> Gd<TileSetSource> {
    let mut source = TileSetAtlasSource::new_gd();
    source.set_texture(&texture);
    let frame_size = metrics.source_frame_size();
    source.set_texture_region_size(Vector2i::new(frame_size, frame_size));
    for mask in 0_u8..16 {
        source.create_tile_ex(road_atlas_coord(mask)).done();
    }
    source.upcast::<TileSetSource>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::buildings::{BuildingBlueprintBundle, ConstructionProgress};
    use game_engine::farming::{FarmInventory, FieldOwner};
    use game_engine::forestry::{ForesterLodgeInventory, TreePlotOwner, TREE_PLOT_GROWTH_TICKS};
    use game_engine::grid::GridSize;
    use game_engine::navigation::{drive_npc_routes, refresh_navigation_snapshot};
    use game_engine::npcs::InitialNpcBundle;
    use game_engine::tile::TileBundle;

    #[test]
    fn selected_npc_route_overlay_reads_remaining_planned_route() {
        let mut world = route_test_world(4, 3);
        let position = NpcPosition::new(CellCoord::new(0, 1));
        let npc = world.spawn(InitialNpcBundle::new(position.coord)).id();
        world
            .entity_mut(npc)
            .insert(NpcRoute::to_cell(CellCoord::new(3, 1)));
        refresh_navigation_snapshot(&mut world);
        drive_npc_routes(&mut world);

        assert_eq!(
            query_selected_npc_route_overlay(&world, npc),
            Some(SelectedNpcRouteOverlay::Route {
                position,
                waypoints: vec![
                    CellCoord::new(1, 1),
                    CellCoord::new(2, 1),
                    CellCoord::new(3, 1),
                ],
                destination: CellCoord::new(3, 1),
            })
        );
    }

    #[test]
    fn selected_npc_route_overlay_reports_blocked_route_at_npc() {
        let mut world = route_test_world(3, 1);
        let blocked_tile = world
            .resource::<TileIndex>()
            .get(CellCoord::new(1, 0))
            .expect("blocked tile should be indexed");
        world.entity_mut(blocked_tile).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 1,
        });
        let position = NpcPosition::new(CellCoord::new(0, 0));
        let npc = world.spawn(InitialNpcBundle::new(position.coord)).id();
        world
            .entity_mut(npc)
            .insert(NpcRoute::to_cell(CellCoord::new(2, 0)));
        refresh_navigation_snapshot(&mut world);
        drive_npc_routes(&mut world);

        assert_eq!(
            query_selected_npc_route_overlay(&world, npc),
            Some(SelectedNpcRouteOverlay::Blocked { position })
        );
    }

    #[test]
    fn selected_npc_route_overlay_ignores_npc_without_route() {
        let mut world = World::new();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(1, 1)))
            .id();

        assert_eq!(query_selected_npc_route_overlay(&world, npc), None);
    }

    #[test]
    fn npc_route_points_start_at_subtile_npc_center_and_use_cell_centers() {
        let position = NpcPosition {
            coord: CellCoord::new(1, 2),
            subtile_offset: SubtileOffset::new(SUBTILE_UNITS_PER_TILE / 2, 0),
        };

        assert_eq!(
            npc_route_points(position, &[CellCoord::new(2, 2), CellCoord::new(2, 1)]),
            vec![
                Vector2::new(128.0, 160.0),
                Vector2::new(160.0, 160.0),
                Vector2::new(160.0, 96.0),
            ]
        );
    }

    #[test]
    fn route_chevrons_point_along_cardinal_and_diagonal_segments() {
        let center = Vector2::new(32.0, 32.0);
        for direction in [
            Vector2::UP,
            Vector2::new(1.0, -1.0),
            Vector2::RIGHT,
            Vector2::new(1.0, 1.0),
            Vector2::DOWN,
            Vector2::new(-1.0, 1.0),
            Vector2::LEFT,
            Vector2::new(-1.0, -1.0),
        ] {
            let to = center + direction * grid::TILE_SIZE;
            let chevron = route_chevron(center, to).expect("segment should produce a chevron");
            let midpoint = (center + to) * 0.5;
            assert!((chevron[1] - midpoint).dot(direction) > 0.0);
            assert!((chevron[0] - midpoint).dot(direction) < 0.0);
            assert!((chevron[2] - midpoint).dot(direction) < 0.0);
        }
        assert_eq!(route_chevron(center, center), None);
    }

    #[test]
    fn npc_animation_name_returns_gather_when_gathering() {
        let npc = npc_render_info_with(Velocity::ZERO, MovementFacing::South, true);

        assert_eq!(npc_animation_name(npc), "gather");
    }

    #[test]
    fn npc_animation_name_prefers_gather_over_walking() {
        let npc = npc_render_info_with(Velocity::new(1, 0), MovementFacing::East, true);

        assert_eq!(npc_animation_name(npc), "gather");
    }

    #[test]
    fn npc_animation_name_maps_refining_activities() {
        for (recipe, expected) in [
            (RecipeKind::SawWood, "saw"),
            (RecipeKind::CutStone, "stonecut"),
            (RecipeKind::CookCrops, "cook"),
            (RecipeKind::CookWildBerries, "cook"),
        ] {
            let mut npc = npc_render_info_with(Velocity::ZERO, MovementFacing::South, false);
            npc.refining_animation = Some(NpcRefiningAnimation::from_recipe(recipe));

            assert_eq!(npc_animation_name(npc), expected);
        }
    }

    #[test]
    fn npc_animation_name_prefers_active_refining_over_other_states() {
        let mut npc = npc_render_info_with(Velocity::new(1, 0), MovementFacing::East, true);
        npc.refining_animation = Some(NpcRefiningAnimation::Cook);

        assert_eq!(npc_animation_name(npc), "cook");
    }

    #[test]
    fn npc_scenes_define_every_refining_animation() {
        let scenes = [
            (
                "botanist",
                include_str!("../../../../godot/world/npc_botanist.tscn"),
            ),
            (
                "colonist",
                include_str!("../../../../godot/world/npc_colonist.tscn"),
            ),
            (
                "engineer",
                include_str!("../../../../godot/world/npc_engineer.tscn"),
            ),
            (
                "miner",
                include_str!("../../../../godot/world/npc_miner.tscn"),
            ),
            (
                "scout",
                include_str!("../../../../godot/world/npc_scout.tscn"),
            ),
        ];

        for (appearance, scene) in scenes {
            for activity in ["saw", "stonecut", "cook"] {
                let asset_path = format!(
                    "res://assets/visual/world/characters/npc_{appearance}_{activity}_sheet.png"
                );
                assert!(scene.contains(asset_path.as_str()));
                assert!(scene.contains(format!("\"name\": &\"{activity}\"").as_str()));
                assert_eq!(
                    scene
                        .matches(
                            format!(
                                "[sub_resource type=\"AtlasTexture\" id=\"AtlasTexture_{activity}_"
                            )
                            .as_str()
                        )
                        .count(),
                    4
                );
            }
            assert!(scene.contains("region = Rect2(768, 0, 256, 256)"));
            assert!(scene.contains("texture_filter = 4"));
            assert!(scene.contains("scale = Vector2(0.25, 0.25)"));
        }
    }

    #[test]
    fn npc_animation_name_returns_idle_when_stationary_and_not_gathering() {
        let npc = npc_render_info_with(Velocity::ZERO, MovementFacing::South, false);

        assert_eq!(npc_animation_name(npc), "idle");
    }

    #[test]
    fn npc_animation_name_returns_directional_walk_when_moving_and_not_gathering() {
        let npc = npc_render_info_with(Velocity::new(1, 0), MovementFacing::East, false);

        assert_eq!(npc_animation_name(npc), "walk_e");
    }

    #[test]
    fn query_npc_render_infos_marks_gathering_npcs() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(2, 3)))
            .id();
        world.entity_mut(npc).insert(AiGatherResource::new(target));

        let infos = query_npc_render_infos(&world);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].entity, npc);
        assert!(infos[0].is_gathering);
    }

    #[test]
    fn query_npc_render_infos_marks_farming_work_as_gathering() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(2, 3)))
            .id();
        world.entity_mut(npc).insert(AiSeedField::new(target));

        let infos = query_npc_render_infos(&world);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].entity, npc);
        assert!(infos[0].is_gathering);
    }

    #[test]
    fn query_npc_render_infos_marks_forestry_work_as_gathering() {
        let mut world = World::new();
        let target = world.spawn_empty().id();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(2, 3)))
            .id();
        world.entity_mut(npc).insert(AiCutTreePlot::new(target));

        let infos = query_npc_render_infos(&world);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].entity, npc);
        assert!(infos[0].is_gathering);
    }

    #[test]
    fn query_npc_render_infos_includes_appearance() {
        let mut world = World::new();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(2, 3)))
            .id();
        world.entity_mut(npc).insert(NpcAppearance::Miner);

        let infos = query_npc_render_infos(&world);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].entity, npc);
        assert_eq!(infos[0].appearance, NpcAppearance::Miner);
    }

    #[test]
    fn query_crop_render_infos_uses_field_crop_state() {
        let mut world = World::new();
        let farm = world
            .spawn((
                Building::new(
                    BuildingKind::Farm,
                    BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
                ),
                FarmInventory::empty(),
            ))
            .id();
        world.spawn((
            Building::new(
                BuildingKind::Field,
                BuildingFootprint::new(CellCoord::new(3, 1), 1, 1),
            ),
            FieldOwner::new(farm),
            FieldCrop::growing(game_engine::farming::FIELD_GROWTH_TICKS),
        ));

        assert_eq!(
            query_crop_render_infos(&world),
            vec![CropRenderInfo {
                coord: CellCoord::new(3, 1),
                state: FieldCropState::Grown,
            }]
        );
    }

    #[test]
    fn query_tree_plot_render_infos_uses_tree_growth_state() {
        let mut world = World::new();
        let lodge = world
            .spawn((
                Building::new(
                    BuildingKind::ForesterLodge,
                    BuildingFootprint::new(CellCoord::new(0, 0), 3, 3),
                ),
                ForesterLodgeInventory::empty(),
            ))
            .id();
        world.spawn((
            Building::new(
                BuildingKind::TreePlot,
                BuildingFootprint::new(CellCoord::new(3, 1), 1, 1),
            ),
            TreePlotOwner::new(lodge),
            TreePlotGrowth::growing(TREE_PLOT_GROWTH_TICKS),
        ));

        assert_eq!(
            query_tree_plot_render_infos(&world),
            vec![TreePlotRenderInfo {
                coord: CellCoord::new(3, 1),
                state: TreePlotState::Mature,
            }]
        );
    }

    #[test]
    fn append_plot_drag_cell_records_path_and_skips_same_cell_samples() {
        let mut cells = Vec::new();

        append_plot_drag_cell(&mut cells, Some(CellCoord::new(3, 2)));
        append_plot_drag_cell(&mut cells, Some(CellCoord::new(3, 2)));
        append_plot_drag_cell(&mut cells, None);
        append_plot_drag_cell(&mut cells, Some(CellCoord::new(4, 2)));

        assert_eq!(cells, vec![CellCoord::new(3, 2), CellCoord::new(4, 2)]);
    }

    #[test]
    fn renderer_transition_cancels_only_active_drag_cells() {
        let mut roads = Some(PlacementMode::Roads {
            tier: RoadTier::DirtPath,
            drag_cells: vec![CellCoord::new(1, 1), CellCoord::new(2, 1)],
            last_rejection: None,
        });
        assert!(cancel_renderer_transition_drag(&mut roads));
        assert!(matches!(
            roads,
            Some(PlacementMode::Roads { ref drag_cells, .. }) if drag_cells.is_empty()
        ));

        let mut building = Some(PlacementMode::Building(BuildingKind::Warehouse));
        assert!(!cancel_renderer_transition_drag(&mut building));
        assert_eq!(
            building,
            Some(PlacementMode::Building(BuildingKind::Warehouse))
        );
    }

    #[test]
    fn unavailable_or_failed_active_3d_renderer_falls_back_to_2d() {
        for availability in [
            Renderer3DAvailability::Preparing,
            Renderer3DAvailability::Failed,
        ] {
            assert_eq!(
                renderer_mode_after_availability_check(RendererMode::ThreeD, availability),
                RendererMode::TwoD
            );
        }
        assert_eq!(
            renderer_mode_after_availability_check(
                RendererMode::ThreeD,
                Renderer3DAvailability::Ready,
            ),
            RendererMode::ThreeD
        );
        assert_eq!(
            renderer_mode_after_availability_check(
                RendererMode::TwoD,
                Renderer3DAvailability::Failed,
            ),
            RendererMode::TwoD
        );
    }

    #[test]
    fn road_drag_fills_horizontal_then_vertical_and_deduplicates() {
        let mut cells = vec![CellCoord::new(1, 1)];
        append_road_drag_cell(&mut cells, Some(CellCoord::new(3, 3)));
        append_road_drag_cell(&mut cells, Some(CellCoord::new(1, 1)));
        assert_eq!(
            cells,
            vec![
                CellCoord::new(1, 1),
                CellCoord::new(2, 1),
                CellCoord::new(3, 1),
                CellCoord::new(3, 2),
                CellCoord::new(3, 3),
                CellCoord::new(2, 3),
                CellCoord::new(1, 3),
                CellCoord::new(1, 2),
            ]
        );
    }

    #[test]
    fn road_masks_use_north_east_south_west_bits_and_row_major_atlas_coords() {
        let center = CellCoord::new(2, 2);
        let cells = [
            center,
            CellCoord::new(2, 1),
            CellCoord::new(3, 2),
            CellCoord::new(2, 3),
            CellCoord::new(1, 2),
        ]
        .into_iter()
        .collect::<HashSet<_>>();
        assert_eq!(road_connectivity_mask(center, &cells), 15);
        assert_eq!(road_atlas_coord(15), Vector2i::new(3, 3));
        assert_eq!(road_atlas_coord(5), Vector2i::new(1, 1));

        let diagonal_cells = [
            center,
            CellCoord::new(1, 1),
            CellCoord::new(3, 1),
            CellCoord::new(1, 3),
            CellCoord::new(3, 3),
        ]
        .into_iter()
        .collect::<HashSet<_>>();
        assert_eq!(road_connectivity_mask(center, &diagonal_cells), 0);
    }

    #[test]
    fn terrain_variants_are_stable_bounded_and_spatially_varied() {
        let simulation = GameSimulation::new(7);
        let surface = simulation.default_surface_id();
        let coord = CellCoord::new(11, 19);
        let first = terrain_variant(surface, coord, TerrainKind::Grass);
        assert_eq!(terrain_variant(surface, coord, TerrainKind::Grass), first);

        let variants = (0..32)
            .flat_map(|y| (0..32).map(move |x| CellCoord::new(x, y)))
            .map(|coord| terrain_variant(surface, coord, TerrainKind::Grass))
            .collect::<HashSet<_>>();
        assert!(variants.iter().all(|variant| (0..4).contains(variant)));
        assert_eq!(variants.len(), TERRAIN_VARIANT_COUNT as usize);
    }

    #[test]
    fn building_visual_scale_preserves_logical_footprint_size() {
        let footprint = BuildingFootprint::new(CellCoord::new(3, 4), 2, 3);
        assert_eq!(
            building_visual_scale(footprint, Vector2::new(512.0, 768.0)),
            Some(Vector2::new(0.25, 0.25))
        );
        assert_eq!(
            building_visual_scale(footprint, Vector2::new(128.0, 192.0)),
            Some(Vector2::ONE)
        );
        assert_eq!(building_visual_scale(footprint, Vector2::ZERO), None);
    }

    #[test]
    fn world_entity_depth_follows_feet_and_stays_in_godot_range() {
        assert!(world_entity_z_index(128.0) < world_entity_z_index(192.0));
        assert_eq!(world_entity_z_index(-100.0), WORLD_ENTITY_Z_BASE);
        assert_eq!(world_entity_z_index(f32::MAX), 4095);
    }

    #[test]
    fn contact_shadow_geometry_is_a_bounded_ellipse() {
        let points = ellipse_points(Vector2::new(14.0, 6.0), 20);
        assert_eq!(points.len(), 20);
        assert!(points
            .iter()
            .all(|point| point.x.abs() <= 14.0 && point.y.abs() <= 6.0));
        assert!(ellipse_points(Vector2::ZERO, 20).is_empty());
        assert!(ellipse_points(Vector2::ONE, 2).is_empty());
    }

    #[test]
    fn building_asset_paths_include_farming_assets() {
        assert_eq!(
            building_asset_path(BuildingKind::Farm),
            "res://assets/visual/world/buildings/building_farm.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::Field),
            "res://assets/visual/world/buildings/building_field.png"
        );
    }

    #[test]
    fn building_asset_paths_include_storage_assets() {
        assert_eq!(
            building_asset_path(BuildingKind::Depot),
            "res://assets/visual/world/buildings/building_depot.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::Warehouse),
            "res://assets/visual/world/buildings/building_warehouse.png"
        );
    }

    #[test]
    fn wheelbarrow_animations_cover_every_load_and_direction() {
        for (facing, direction) in [
            (MovementFacing::North, "n"),
            (MovementFacing::NorthEast, "ne"),
            (MovementFacing::East, "e"),
            (MovementFacing::SouthEast, "se"),
            (MovementFacing::South, "s"),
            (MovementFacing::SouthWest, "sw"),
            (MovementFacing::West, "w"),
            (MovementFacing::NorthWest, "nw"),
        ] {
            assert_eq!(
                wheelbarrow_animation_name(facing, None),
                format!("empty_{direction}")
            );
            for kind in ResourceKind::ALL {
                assert_eq!(
                    wheelbarrow_animation_name(facing, Some(kind)),
                    format!("{}_{direction}", resource_animation_slug(kind))
                );
            }
        }
    }

    #[test]
    fn wheelbarrow_transform_places_it_ahead_of_the_npc() {
        for (facing, expected_position, expected_z_index) in [
            (MovementFacing::North, Vector2::new(0.0, -96.0), -1),
            (MovementFacing::NorthEast, Vector2::new(72.0, -72.0), -1),
            (MovementFacing::East, Vector2::new(96.0, 0.0), 1),
            (MovementFacing::SouthEast, Vector2::new(72.0, 72.0), 1),
            (MovementFacing::South, Vector2::new(0.0, 96.0), 1),
            (MovementFacing::SouthWest, Vector2::new(-72.0, 72.0), 1),
            (MovementFacing::West, Vector2::new(-96.0, 0.0), 1),
            (MovementFacing::NorthWest, Vector2::new(-72.0, -72.0), -1),
        ] {
            assert_eq!(
                wheelbarrow_transform(facing),
                (expected_position, expected_z_index)
            );
        }
    }

    #[test]
    fn wheelbarrow_assets_cover_every_resource_kind() {
        for kind in ResourceKind::ALL {
            let path = wheelbarrow_asset_path(kind);
            assert!(path.starts_with("res://assets/visual/world/vehicles/wheelbarrow_"));
            assert!(path.ends_with("_sheet.png"));
        }
    }

    #[test]
    fn npc_render_info_distinguishes_empty_and_loaded_wheelbarrows() {
        let mut world = World::new();
        let empty = world
            .spawn((
                InitialNpcBundle::new(CellCoord::new(1, 2)),
                Wheelbarrow::empty(),
            ))
            .id();
        let loaded = world
            .spawn((
                InitialNpcBundle::new(CellCoord::new(3, 4)),
                Wheelbarrow::of(ResourceKind::Planks, 12),
            ))
            .id();

        let infos = query_npc_render_infos(&world);
        let empty_info = infos
            .iter()
            .find(|info| info.entity == empty)
            .expect("empty wheelbarrow NPC should render");
        assert!(empty_info.has_wheelbarrow);
        assert_eq!(empty_info.wheelbarrow_kind, None);
        let loaded_info = infos
            .iter()
            .find(|info| info.entity == loaded)
            .expect("loaded wheelbarrow NPC should render");
        assert!(loaded_info.has_wheelbarrow);
        assert_eq!(loaded_info.wheelbarrow_kind, Some(ResourceKind::Planks));
    }

    #[test]
    fn building_asset_paths_include_refinery_assets() {
        assert_eq!(
            building_asset_path(BuildingKind::Sawmill),
            "res://assets/visual/world/buildings/building_sawmill.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::Stoneworks),
            "res://assets/visual/world/buildings/building_stoneworks.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::Kitchen),
            "res://assets/visual/world/buildings/building_kitchen.png"
        );
    }

    #[test]
    fn building_asset_paths_include_forestry_assets() {
        assert_eq!(
            building_asset_path(BuildingKind::ForesterLodge),
            "res://assets/visual/world/buildings/building_forester_lodge.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::TreePlot),
            "res://assets/visual/world/buildings/building_tree_plot.png"
        );
    }

    #[test]
    fn building_asset_paths_include_housing_assets() {
        assert_eq!(
            building_asset_path(BuildingKind::SmallHouse),
            "res://assets/visual/world/buildings/building_house_small.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::MediumHouse),
            "res://assets/visual/world/buildings/building_house_medium.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::LargeHouse),
            "res://assets/visual/world/buildings/building_house_large.png"
        );
    }

    #[test]
    fn crop_source_ids_use_seedable_tile_for_active_seeding() {
        assert_eq!(crop_source_id(FieldCropState::Seedable), 0);
        assert_eq!(crop_source_id(FieldCropState::Seeding), 0);
        assert_eq!(crop_render_source_id(FieldCropState::Inactive), None);
        assert_eq!(crop_render_source_id(FieldCropState::Grown), Some(3));
    }

    #[test]
    fn tree_plot_source_ids_only_render_growth_overlays() {
        assert_eq!(tree_plot_render_source_id(TreePlotState::Inactive), None);
        assert_eq!(tree_plot_render_source_id(TreePlotState::Seedable), None);
        assert_eq!(tree_plot_render_source_id(TreePlotState::Seeding), None);
        assert_eq!(tree_plot_render_source_id(TreePlotState::Sapling), Some(0));
        assert_eq!(tree_plot_render_source_id(TreePlotState::Young), Some(1));
        assert_eq!(tree_plot_render_source_id(TreePlotState::Mature), Some(2));
    }

    #[test]
    fn tile_only_click_selects_tile_and_clears_npc_and_building() {
        let coord = CellCoord::new(2, 3);
        let mut world = world_with_tiles(&[coord]);
        let tile = indexed_tile_entity(&world, coord);
        let previous_npc_coord = CellCoord::new(5, 5);
        let previous_npc = world.spawn(InitialNpcBundle::new(previous_npc_coord)).id();
        let previous_footprint = BuildingFootprint::new(CellCoord::new(6, 6), 2, 2);
        let previous_building = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                previous_footprint,
            ))
            .id();

        let mut selected_cell = None;
        let mut selected_npc = Some(SelectedNpc {
            coord: previous_npc_coord,
            entity: previous_npc,
        });
        let mut selected_building = Some(SelectedBuilding {
            footprint: previous_footprint,
            entity: previous_building,
        });

        let targets = click_selection_targets_at(&world, coord);
        let events = apply_click_selection_targets(
            &mut selected_cell,
            &mut selected_npc,
            &mut selected_building,
            targets,
        );

        assert_eq!(
            selected_cell,
            Some(SelectedCell {
                coord,
                entity: tile
            })
        );
        assert_eq!(selected_npc, None);
        assert_eq!(selected_building, None);
        assert_eq!(
            events,
            vec![
                SelectionEvent::NpcDeselected,
                SelectionEvent::BuildingDeselected,
                SelectionEvent::TileSelected(tile),
            ]
        );
    }

    #[test]
    fn populated_cell_click_selects_tile_npc_and_building() {
        let coord = CellCoord::new(2, 2);
        let mut world = world_with_tiles(&[coord]);
        let tile = indexed_tile_entity(&world, coord);
        let npc = world.spawn(InitialNpcBundle::new(coord)).id();
        let footprint = BuildingFootprint::new(CellCoord::new(2, 2), 2, 2);
        let building = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                footprint,
            ))
            .id();

        let mut selected_cell = None;
        let mut selected_npc = None;
        let mut selected_building = None;

        let targets = click_selection_targets_at(&world, coord);
        let events = apply_click_selection_targets(
            &mut selected_cell,
            &mut selected_npc,
            &mut selected_building,
            targets,
        );

        assert_eq!(
            selected_cell,
            Some(SelectedCell {
                coord,
                entity: tile
            })
        );
        assert_eq!(selected_npc, Some(SelectedNpc { coord, entity: npc }));
        assert_eq!(
            selected_building,
            Some(SelectedBuilding {
                footprint,
                entity: building,
            })
        );
        assert_eq!(
            events,
            vec![
                SelectionEvent::TileSelected(tile),
                SelectionEvent::NpcSelected(npc),
                SelectionEvent::BuildingSelected(building),
            ]
        );
    }

    #[test]
    fn repeated_populated_cell_click_reemits_selected_events() {
        let coord = CellCoord::new(2, 2);
        let mut world = world_with_tiles(&[coord]);
        let tile = indexed_tile_entity(&world, coord);
        let npc = world.spawn(InitialNpcBundle::new(coord)).id();
        let footprint = BuildingFootprint::new(CellCoord::new(2, 2), 2, 2);
        let building = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                footprint,
            ))
            .id();

        let mut selected_cell = None;
        let mut selected_npc = None;
        let mut selected_building = None;
        let targets = click_selection_targets_at(&world, coord);
        apply_click_selection_targets(
            &mut selected_cell,
            &mut selected_npc,
            &mut selected_building,
            targets,
        );

        let events = apply_click_selection_targets(
            &mut selected_cell,
            &mut selected_npc,
            &mut selected_building,
            targets,
        );

        assert_eq!(
            selected_cell,
            Some(SelectedCell {
                coord,
                entity: tile
            })
        );
        assert_eq!(selected_npc, Some(SelectedNpc { coord, entity: npc }));
        assert_eq!(
            selected_building,
            Some(SelectedBuilding {
                footprint,
                entity: building,
            })
        );
        assert_eq!(
            events,
            vec![
                SelectionEvent::TileSelected(tile),
                SelectionEvent::NpcSelected(npc),
                SelectionEvent::BuildingSelected(building),
            ]
        );
    }

    #[test]
    fn clicking_original_tile_after_selected_npc_moves_away_clears_npc_selection() {
        let original_coord = CellCoord::new(2, 2);
        let moved_coord = CellCoord::new(3, 2);
        let mut world = world_with_tiles(&[original_coord, moved_coord]);
        let tile = indexed_tile_entity(&world, original_coord);
        let npc = world.spawn(InitialNpcBundle::new(original_coord)).id();
        world
            .get_mut::<NpcPosition>(npc)
            .expect("spawned NPC should have a position")
            .coord = moved_coord;

        let mut selected_cell = Some(SelectedCell {
            coord: original_coord,
            entity: tile,
        });
        let mut selected_npc = Some(SelectedNpc {
            coord: original_coord,
            entity: npc,
        });
        let mut selected_building = None;

        let targets = click_selection_targets_at(&world, original_coord);
        let events = apply_click_selection_targets(
            &mut selected_cell,
            &mut selected_npc,
            &mut selected_building,
            targets,
        );

        assert_eq!(
            selected_cell,
            Some(SelectedCell {
                coord: original_coord,
                entity: tile,
            })
        );
        assert_eq!(selected_npc, None);
        assert_eq!(
            events,
            vec![
                SelectionEvent::NpcDeselected,
                SelectionEvent::TileSelected(tile),
            ]
        );
    }

    #[test]
    fn multiple_npcs_on_cell_select_lowest_entity_bits() {
        let coord = CellCoord::new(2, 2);
        let mut world = world_with_tiles(&[coord]);
        let first = world.spawn(InitialNpcBundle::new(coord)).id();
        let second = world.spawn(InitialNpcBundle::new(coord)).id();
        let expected = [first, second]
            .into_iter()
            .min_by_key(|entity| entity.to_bits())
            .expect("at least one NPC should exist");

        let targets = click_selection_targets_at(&world, coord);

        assert_eq!(
            targets.npc,
            Some(SelectedNpc {
                coord,
                entity: expected,
            })
        );
    }

    #[test]
    fn building_selection_resolves_from_non_origin_footprint_cell() {
        let coord = CellCoord::new(3, 3);
        let mut world = world_with_tiles(&[coord]);
        let footprint = BuildingFootprint::new(CellCoord::new(2, 2), 2, 2);
        let building = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                footprint,
            ))
            .id();

        let targets = click_selection_targets_at(&world, coord);

        assert_eq!(
            targets.building,
            Some(SelectedBuilding {
                footprint,
                entity: building,
            })
        );
    }

    #[test]
    fn map_entity_target_returns_resource_node_for_resource_cell() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        let resource = spawn_resource_node(&mut world, coord);

        assert_eq!(
            map_entity_target_at(&world, coord),
            Some(MapEntityTarget {
                kind: MapEntityKind::ResourceNode,
                entity: resource,
            })
        );
    }

    #[test]
    fn proxy_target_revalidates_each_supported_entity_kind() {
        let mut world = World::new();
        let building = world
            .spawn(Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(1, 1), 2, 2),
            ))
            .id();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(3, 3)))
            .id();
        let resource = spawn_resource_node(&mut world, CellCoord::new(4, 4));
        let road = world
            .spawn(RoadBlueprint {
                coord: CellCoord::new(5, 5),
                target_tier: RoadTier::DirtPath,
            })
            .id();

        for (proxy_kind, entity, expected_kind) in [
            (ProxyKind3D::Building, building, MapEntityKind::Building),
            (ProxyKind3D::Npc, npc, MapEntityKind::Npc),
            (ProxyKind3D::Resource, resource, MapEntityKind::ResourceNode),
            (
                ProxyKind3D::RoadBlueprint,
                road,
                MapEntityKind::RoadBlueprint,
            ),
        ] {
            assert_eq!(
                map_entity_target_from_proxy(
                    &world,
                    ProxyHit3D {
                        kind: proxy_kind,
                        entity,
                        distance: 1.0,
                    },
                ),
                Some(MapEntityTarget {
                    kind: expected_kind,
                    entity,
                })
            );
        }
    }

    #[test]
    fn proxy_target_rejects_mismatched_and_despawned_entities() {
        let mut world = World::new();
        let npc = world
            .spawn(InitialNpcBundle::new(CellCoord::new(2, 3)))
            .id();
        let resource_hit = ProxyHit3D {
            kind: ProxyKind3D::Resource,
            entity: npc,
            distance: 1.0,
        };

        assert_eq!(map_entity_target_from_proxy(&world, resource_hit), None);

        let npc_hit = ProxyHit3D {
            kind: ProxyKind3D::Npc,
            entity: npc,
            distance: 1.0,
        };
        world.despawn(npc);

        assert_eq!(map_entity_target_from_proxy(&world, npc_hit), None);
    }

    #[test]
    fn proxy_selection_records_use_current_component_data() {
        let mut world = World::new();
        let npc_coord = CellCoord::new(7, 8);
        let npc = world.spawn(InitialNpcBundle::new(npc_coord)).id();
        let footprint = BuildingFootprint::new(CellCoord::new(9, 10), 3, 2);
        let building = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                footprint,
            ))
            .id();

        assert_eq!(
            selected_npc_for_entity(&world, npc),
            Some(SelectedNpc {
                coord: npc_coord,
                entity: npc,
            })
        );
        assert_eq!(
            selected_building_for_entity(&world, building),
            Some(SelectedBuilding {
                footprint,
                entity: building,
            })
        );
    }

    #[test]
    fn map_entity_kind_signal_values_round_trip() {
        for (kind, value) in [
            (MapEntityKind::Building, 0),
            (MapEntityKind::Npc, 1),
            (MapEntityKind::ResourceNode, 2),
            (MapEntityKind::RoadBlueprint, 3),
        ] {
            assert_eq!(kind.signal_value(), value);
            assert_eq!(MapEntityKind::from_signal_value(value), Some(kind));
        }
        assert_eq!(MapEntityKind::from_signal_value(4), None);
    }

    #[test]
    fn map_entity_target_returns_road_blueprint_for_pending_road_cell() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        let road = world
            .spawn((
                RoadBlueprint {
                    coord,
                    target_tier: RoadTier::Cobblestone,
                },
                ConstructionProgress::new(ResourceAmounts::zero()).with_required_labor(180),
            ))
            .id();

        assert_eq!(
            map_entity_target_at(&world, coord),
            Some(MapEntityTarget {
                kind: MapEntityKind::RoadBlueprint,
                entity: road,
            })
        );
    }

    #[test]
    fn map_entity_target_prioritizes_road_blueprint_over_npc() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        world.spawn(InitialNpcBundle::new(coord));
        let road = world
            .spawn((
                RoadBlueprint {
                    coord,
                    target_tier: RoadTier::DirtPath,
                },
                ConstructionProgress::new(ResourceAmounts::zero()).with_required_labor(180),
            ))
            .id();

        assert_eq!(
            map_entity_target_at(&world, coord),
            Some(MapEntityTarget {
                kind: MapEntityKind::RoadBlueprint,
                entity: road,
            })
        );
    }

    #[test]
    fn map_entity_target_does_not_target_completed_road() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        world.spawn(Road {
            coord,
            tier: RoadTier::DirtPath,
        });

        assert_eq!(map_entity_target_at(&world, coord), None);
    }

    #[test]
    fn map_entity_target_prioritizes_npc_over_resource_node() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        spawn_resource_node(&mut world, coord);
        let npc = world.spawn(InitialNpcBundle::new(coord)).id();

        assert_eq!(
            map_entity_target_at(&world, coord),
            Some(MapEntityTarget {
                kind: MapEntityKind::Npc,
                entity: npc,
            })
        );
    }

    #[test]
    fn map_entity_target_prioritizes_building_over_npc_and_resource_node() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        spawn_resource_node(&mut world, coord);
        world.spawn(InitialNpcBundle::new(coord));
        let building = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
            ))
            .id();

        assert_eq!(
            map_entity_target_at(&world, coord),
            Some(MapEntityTarget {
                kind: MapEntityKind::Building,
                entity: building,
            })
        );
    }

    #[test]
    fn map_entity_target_prioritizes_finished_building_over_npc_and_resource_node() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        spawn_resource_node(&mut world, coord);
        world.spawn(InitialNpcBundle::new(coord));
        let building = world
            .spawn(Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
            ))
            .id();

        assert_eq!(
            map_entity_target_at(&world, coord),
            Some(MapEntityTarget {
                kind: MapEntityKind::Building,
                entity: building,
            })
        );
    }

    #[test]
    fn query_building_render_infos_includes_finished_buildings() {
        let mut world = World::new();
        let building = world
            .spawn(Building::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
            ))
            .id();

        assert_eq!(
            query_building_render_infos(&world),
            vec![BuildingRenderInfo {
                entity: building,
                kind: BuildingKind::Warehouse,
                footprint: BuildingFootprint::new(CellCoord::new(2, 2), 2, 2),
                state: BuildingRenderState::Constructed,
            }]
        );
    }

    #[test]
    fn query_building_render_infos_marks_blueprints_and_constructed_buildings() {
        let mut world = World::new();
        let blueprint_footprint = BuildingFootprint::new(CellCoord::new(1, 1), 2, 2);
        let blueprint = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                blueprint_footprint,
            ))
            .id();
        let constructed_footprint = BuildingFootprint::new(CellCoord::new(5, 5), 3, 3);
        let constructed = world
            .spawn(Building::new(BuildingKind::TownHall, constructed_footprint))
            .id();

        let mut expected = vec![
            BuildingRenderInfo {
                entity: blueprint,
                kind: BuildingKind::Warehouse,
                footprint: blueprint_footprint,
                state: BuildingRenderState::Blueprint,
            },
            BuildingRenderInfo {
                entity: constructed,
                kind: BuildingKind::TownHall,
                footprint: constructed_footprint,
                state: BuildingRenderState::Constructed,
            },
        ];
        expected.sort_by_key(|building| building.entity.to_bits());

        assert_eq!(query_building_render_infos(&world), expected);
    }

    #[test]
    fn building_sprite_modulate_distinguishes_blueprint_and_constructed_states() {
        let blueprint = building_sprite_modulate(BuildingRenderState::Blueprint);
        assert_eq!(blueprint, {
            let mut color = Color::from_rgb(0.55, 0.9, 1.0);
            color.a = 0.62;
            color
        });

        assert_eq!(
            building_sprite_modulate(BuildingRenderState::Constructed),
            Color::from_rgb(1.0, 1.0, 1.0)
        );
    }

    #[test]
    fn map_entity_target_returns_none_for_empty_cell() {
        let mut world = World::new();
        spawn_resource_node(&mut world, CellCoord::new(2, 3));

        assert_eq!(map_entity_target_at(&world, CellCoord::new(4, 5)), None);
    }

    fn npc_render_info_with(
        velocity: Velocity,
        facing: MovementFacing,
        is_gathering: bool,
    ) -> NpcRenderInfo {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        NpcRenderInfo {
            entity,
            appearance: NpcAppearance::Colonist,
            coord: CellCoord::new(0, 0),
            subtile_offset: SubtileOffset::ZERO,
            velocity,
            facing,
            is_gathering,
            refining_animation: None,
            carried_kind: None,
            has_wheelbarrow: false,
            wheelbarrow_kind: None,
        }
    }

    fn spawn_resource_node(world: &mut World, coord: CellCoord) -> Entity {
        let entity = world.spawn(TileBundle::new(coord)).id();
        world.entity_mut(entity).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 100,
        });
        entity
    }

    fn world_with_tiles(coords: &[CellCoord]) -> World {
        let mut world = World::new();
        let mut index = TileIndex::new(GridSize::new(8, 8));
        for &coord in coords {
            let entity = world.spawn(TileBundle::new(coord)).id();
            assert!(index.set(coord, entity));
        }
        world.insert_resource(index);
        world
    }

    fn route_test_world(width: usize, height: usize) -> World {
        let size = GridSize::new(width, height);
        let mut world = World::new();
        world.insert_resource(Grid::new(width, height));
        let mut index = TileIndex::new(size);
        for coord in size.iter_coords() {
            let entity = world
                .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Grass))
                .id();
            assert!(index.set(coord, entity));
        }
        world.insert_resource(index);
        world
    }

    fn indexed_tile_entity(world: &World, coord: CellCoord) -> Entity {
        world
            .resource::<TileIndex>()
            .get(coord)
            .expect("test tile should exist in index")
    }
}
