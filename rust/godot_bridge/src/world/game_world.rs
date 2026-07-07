use crate::assets::{
    load_packed_scene, load_texture, npc_scene_path, resource_asset_path, terrain_asset_path,
};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
};
use game_engine::components::{
    AiGatherResource, MovementFacing, NpcAppearance, SubtileOffset, TerrainKind, Tile,
    TilePosition, Velocity, SUBTILE_UNITS_PER_TILE,
};
use game_engine::farming::{
    field_crop_state, AiHarvestField, AiSeedField, FieldCrop, FieldCropState,
    FieldPlacementPreview, HarvestField, SeedField,
};
use game_engine::grid::{self, CellCoord, Grid, WorldPosition};
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::simulation::{GameSimulation, SimulationSpeed, SurfaceId};
use game_engine::tasks::ProgressBuildingConstruction;
use game_engine::tile::TileIndex;
use godot::builtin::Side;
use godot::classes::{
    canvas_item::TextureFilter, AnimatedSprite2D, Camera2D, INode2D, Input, InputEvent,
    InputEventMouseButton, Node2D, PackedScene, Sprite2D, Texture2D, TileMapLayer, TileSet,
    TileSetAtlasSource, TileSetSource,
};
use godot::global::MouseButton;
use godot::obj::{OnEditor, Singleton};
use godot::prelude::*;
use std::collections::{HashMap, HashSet};

const ZOOM_ABSOLUTE_FLOOR: f32 = 0.001;
const ZOOM_MARGIN: f32 = 0.95;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_FACTOR: f32 = 1.1;
const PAN_SPEED: f32 = 600.0;
const CAMERA_LIMIT_PADDING_FACTOR: f32 = 1.0;
const ACTION_CAMERA_PAN_UP: &str = "camera_pan_up";
const ACTION_CAMERA_PAN_DOWN: &str = "camera_pan_down";
const ACTION_CAMERA_PAN_LEFT: &str = "camera_pan_left";
const ACTION_CAMERA_PAN_RIGHT: &str = "camera_pan_right";
const ACTION_MENU_TOGGLE: &str = "menu_toggle";
const BUILDING_WAREHOUSE_PATH: &str = "res://assets/generated/building_warehouse.png";
const BUILDING_TOWNHALL_PATH: &str = "res://assets/generated/building_townhall.png";
const BUILDING_FARM_PATH: &str = "res://assets/generated/building_farm.png";
const BUILDING_FIELD_PATH: &str = "res://assets/generated/building_field.png";
const CROP_SEEDABLE_PATH: &str = "res://assets/generated/crop_seedable_plot.png";
const CROP_GROWING_STEP1_PATH: &str = "res://assets/generated/crop_growing_step1.png";
const CROP_GROWING_STEP2_PATH: &str = "res://assets/generated/crop_growing_step2.png";
const CROP_GROWN_PATH: &str = "res://assets/generated/crop_grown.png";

fn world_limit(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }

    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapEntityKind {
    Building,
    Npc,
    ResourceNode,
}

impl MapEntityKind {
    pub(crate) const fn signal_value(self) -> i64 {
        match self {
            Self::Building => 0,
            Self::Npc => 1,
            Self::ResourceNode => 2,
        }
    }

    pub(crate) const fn from_signal_value(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Building),
            1 => Some(Self::Npc),
            2 => Some(Self::ResourceNode),
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
struct NpcRenderInfo {
    entity: Entity,
    appearance: NpcAppearance,
    coord: CellCoord,
    subtile_offset: SubtileOffset,
    velocity: Velocity,
    facing: MovementFacing,
    is_gathering: bool,
}

struct RenderedNpcSprite {
    appearance: NpcAppearance,
    sprite: Gd<AnimatedSprite2D>,
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
enum BuildingRenderState {
    Blueprint,
    Constructed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlacementMode {
    Building(BuildingKind),
    Fields {
        farm: Entity,
        drag_cells: Vec<CellCoord>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskTableRow {
    pub(crate) entity_id: i64,
    pub(crate) task_type: String,
    pub(crate) details: String,
}

#[derive(GodotClass)]
#[class(base = Node2D)]
pub(crate) struct GameWorld {
    #[export]
    tile_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    resource_node_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    crop_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    camera: OnEditor<Gd<Camera2D>>,

    game: GameSimulation,
    rendered_surface: SurfaceId,
    selected_cell: Option<SelectedCell>,
    selected_npc: Option<SelectedNpc>,
    selected_building: Option<SelectedBuilding>,
    hovered_map_entity: Option<MapEntityTarget>,
    placement_mode: Option<PlacementMode>,
    _tile_set: Option<Gd<TileSet>>,
    _resource_node_tile_set: Option<Gd<TileSet>>,
    _crop_tile_set: Option<Gd<TileSet>>,
    npc_scenes: HashMap<NpcAppearance, Gd<PackedScene>>,
    npc_sprites: HashMap<Entity, RenderedNpcSprite>,
    building_textures: HashMap<BuildingKind, Gd<Texture2D>>,
    building_sprites: HashMap<Entity, Gd<Sprite2D>>,

    base: Base<Node2D>,
}

#[godot_api]
impl INode2D for GameWorld {
    fn init(base: Base<Node2D>) -> Self {
        let game = GameSimulation::new();
        let rendered_surface = game.default_surface_id();

        Self {
            tile_map: OnEditor::default(),
            resource_node_map: OnEditor::default(),
            crop_map: OnEditor::default(),
            camera: OnEditor::default(),
            game,
            rendered_surface,
            selected_cell: None,
            selected_npc: None,
            selected_building: None,
            hovered_map_entity: None,
            placement_mode: None,
            _tile_set: None,
            _resource_node_tile_set: None,
            _crop_tile_set: None,
            npc_scenes: HashMap::new(),
            npc_sprites: HashMap::new(),
            building_textures: HashMap::new(),
            building_sprites: HashMap::new(),
            base,
        }
    }

    fn ready(&mut self) {
        let mut tm = self.tile_map.clone();
        let mut resource_map = self.resource_node_map.clone();
        let mut crop_map = self.crop_map.clone();
        let mut cam = self.camera.clone();

        let ts = grid::TILE_SIZE as i32;
        let Some(tile_set) = self.build_terrain_tile_set(ts) else {
            self.disable_processing();
            return;
        };

        tm.set_tile_set(&tile_set);
        self._tile_set = Some(tile_set);
        tm.set_navigation_enabled(false);
        tm.set_texture_filter(TextureFilter::NEAREST);
        tm.set_draw_behind_parent(true);

        if !self.populate_tile_map(&mut tm) {
            self.disable_processing();
            return;
        }

        if !self.load_building_textures() {
            self.disable_processing();
            return;
        }
        self.sync_building_sprites();

        let Some(resource_tile_set) = self.build_resource_node_tile_set(ts) else {
            self.disable_processing();
            return;
        };
        resource_map.set_tile_set(&resource_tile_set);
        self._resource_node_tile_set = Some(resource_tile_set);
        resource_map.set_navigation_enabled(false);
        resource_map.set_texture_filter(TextureFilter::NEAREST);
        resource_map.set_z_index(1);
        self.populate_resource_node_map(&mut resource_map);

        let Some(crop_tile_set) = self.build_crop_tile_set(ts) else {
            self.disable_processing();
            return;
        };
        crop_map.set_tile_set(&crop_tile_set);
        self._crop_tile_set = Some(crop_tile_set);
        crop_map.set_navigation_enabled(false);
        crop_map.set_texture_filter(TextureFilter::NEAREST);
        crop_map.set_z_index(3);
        self.populate_crop_map(&mut crop_map);

        if !self.load_npc_scenes() {
            self.disable_processing();
            return;
        }
        self.sync_npc_sprites();

        cam.set_enabled(true);
        cam.make_current();
        cam.set_zoom(Vector2::new(0.5, 0.5));
        self.configure_camera_for_surface(&mut cam);

        self.base_mut().set_process_input(true);
        self.base_mut().set_process(true);
        self.base_mut().queue_redraw();
    }

    fn process(&mut self, delta: f64) {
        let input = Input::singleton();
        let mut cam = self.camera.clone();

        let vs = self.get_viewport_size();
        let ws = self.world_size();
        let min_zoom = {
            let fit_x = vs.x / ws.x;
            let fit_y = vs.y / ws.y;
            (fit_x.max(fit_y) * ZOOM_MARGIN).max(ZOOM_ABSOLUTE_FLOOR)
        };

        let zoom = cam.get_zoom().x;
        let clamped = if zoom < min_zoom {
            min_zoom
        } else if zoom > ZOOM_MAX {
            ZOOM_MAX
        } else {
            zoom
        };
        if (clamped - zoom).abs() > f32::EPSILON {
            cam.set_zoom(Vector2::new(clamped, clamped));
        }

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

        if dir != Vector2::ZERO {
            dir = dir.normalized();
            let zoom = cam.get_zoom().x;
            let speed = PAN_SPEED / zoom;
            let pos = cam.get_position();
            cam.set_position(pos + dir * speed * delta as f32);
        }

        self.game.tick();
        let mut resource_map = self.resource_node_map.clone();
        self.populate_resource_node_map(&mut resource_map);
        let mut crop_map = self.crop_map.clone();
        self.populate_crop_map(&mut crop_map);
        let buildings_changed = self.sync_building_sprites();
        self.sync_npc_sprites();
        self.update_field_drag_current();
        if self.placement_mode.is_some() || buildings_changed {
            self.base_mut().queue_redraw();
        }
        self.update_hovered_map_entity();
    }

    fn draw(&mut self) {
        let ts = grid::TILE_SIZE;
        let ws = self.world_size();
        let size = self.grid_size();
        let Some(w) = size.width_i32() else {
            godot_warn!("GameWorld: grid width is too large to draw");
            return;
        };
        let Some(h) = size.height_i32() else {
            godot_warn!("GameWorld: grid height is too large to draw");
            return;
        };
        let grid_color = Color::from_rgb(0.12, 0.35, 0.05);
        let selected_cell = self.selected_cell;
        let selected_npc = self.selected_npc;
        let selected_building = self.selected_building;
        let building_preview = self.building_preview();
        let field_previews = self.field_previews();
        let blueprint_footprints = self
            .building_render_infos()
            .into_iter()
            .filter(|building| building.state == BuildingRenderState::Blueprint)
            .map(|building| building.footprint)
            .collect::<Vec<_>>();

        let mut base = self.base_mut();
        for x in 0..=w {
            let px = x as f32 * ts;
            base.draw_line(Vector2::new(px, 0.0), Vector2::new(px, ws.y), grid_color);
        }
        for y in 0..=h {
            let py = y as f32 * ts;
            base.draw_line(Vector2::new(0.0, py), Vector2::new(ws.x, py), grid_color);
        }

        base.draw_rect_ex(
            Rect2::new(Vector2::ZERO, ws),
            Color::from_rgb(0.95, 0.35, 0.05),
        )
        .filled(false)
        .width(8.0)
        .done();

        for footprint in blueprint_footprints {
            let highlight = Color::from_rgb(0.15, 0.85, 1.0);
            let mut fill = highlight;
            fill.a = 0.14;
            let rect = footprint_rect(footprint);
            base.draw_rect_ex(rect, fill).filled(true).done();
            base.draw_rect_ex(rect, highlight)
                .filled(false)
                .width(3.0)
                .done();
        }

        if let Some(selected) = selected_cell {
            let coord = selected.coord;
            let cell_pos = Vector2::new(coord.x() as f32 * ts, coord.y() as f32 * ts);
            let cell_size = Vector2::new(ts, ts);
            let highlight = Color::from_rgb(1.0, 0.84, 0.0);
            let mut fill = highlight;
            fill.a = 0.15;
            base.draw_rect_ex(Rect2::new(cell_pos, cell_size), fill)
                .filled(true)
                .done();
            base.draw_rect_ex(Rect2::new(cell_pos, cell_size), highlight)
                .filled(false)
                .width(4.0)
                .done();
        }

        if let Some(selected) = selected_npc {
            let coord = selected.coord;
            let cell_pos = Vector2::new(coord.x() as f32 * ts, coord.y() as f32 * ts);
            let cell_size = Vector2::new(ts, ts);
            let highlight = Color::from_rgb(0.1, 0.85, 1.0);
            let mut fill = highlight;
            fill.a = 0.12;
            base.draw_rect_ex(Rect2::new(cell_pos, cell_size), fill)
                .filled(true)
                .done();
            base.draw_rect_ex(Rect2::new(cell_pos, cell_size), highlight)
                .filled(false)
                .width(4.0)
                .done();
        }

        if let Some(selected) = selected_building {
            let highlight = Color::from_rgb(1.0, 0.55, 0.12);
            let mut fill = highlight;
            fill.a = 0.10;
            let rect = footprint_rect(selected.footprint);
            base.draw_rect_ex(rect, fill).filled(true).done();
            base.draw_rect_ex(rect, highlight)
                .filled(false)
                .width(4.0)
                .done();
        }

        for preview in field_previews {
            let color = if preview.result.is_ok() {
                Color::from_rgb(0.1, 0.9, 0.45)
            } else {
                Color::from_rgb(1.0, 0.1, 0.1)
            };
            let mut fill = color;
            fill.a = 0.18;
            let rect = Rect2::new(
                cell_top_left(preview.coord),
                Vector2::new(grid::TILE_SIZE, grid::TILE_SIZE),
            );
            base.draw_rect_ex(rect, fill).filled(true).done();
            base.draw_rect_ex(rect, color)
                .filled(false)
                .width(4.0)
                .done();
        }

        if let Some((footprint, valid)) = building_preview {
            let color = if valid {
                Color::from_rgb(0.1, 0.9, 0.45)
            } else {
                Color::from_rgb(1.0, 0.1, 0.1)
            };
            let mut fill = color;
            fill.a = 0.18;
            let rect = footprint_rect(footprint);
            base.draw_rect_ex(rect, fill).filled(true).done();
            base.draw_rect_ex(rect, color)
                .filled(false)
                .width(4.0)
                .done();
        }
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if event.is_action_pressed(ACTION_MENU_TOGGLE) && self.placement_mode.is_some() {
            self.cancel_placement_mode();
            self.mark_input_handled();
            return;
        }

        let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() else {
            return;
        };

        match mouse.get_button_index() {
            MouseButton::LEFT => {
                if mouse.is_pressed() {
                    self.handle_primary_press();
                } else {
                    self.handle_primary_release();
                }
            }
            MouseButton::RIGHT => {
                if mouse.is_pressed() && self.placement_mode.is_some() {
                    self.cancel_placement_mode();
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
        let coords = self.game.tile_coords(self.rendered_surface);

        for coord in coords {
            let Some(terrain) = self.game.tile_terrain_at(self.rendered_surface, coord) else {
                godot_error!(
                    "GameWorld: terrain missing for tile at ({}, {})",
                    coord.x(),
                    coord.y()
                );
                return false;
            };

            tile_map
                .set_cell_ex(v2(coord.x(), coord.y()))
                .source_id(terrain_source_id(terrain))
                .atlas_coords(v2(0, 0))
                .done();
        }
        tile_map.update_internals();
        true
    }

    fn configure_camera_for_surface(&self, camera: &mut Gd<Camera2D>) {
        let world_size = self.world_size();
        let padding = world_size * CAMERA_LIMIT_PADDING_FACTOR;
        camera.set_position(world_size / 2.0);
        camera.set_limit(Side::LEFT, world_limit(-padding.x));
        camera.set_limit(Side::TOP, world_limit(-padding.y));
        camera.set_limit(Side::RIGHT, world_limit(world_size.x + padding.x));
        camera.set_limit(Side::BOTTOM, world_limit(world_size.y + padding.y));
        camera.set_limit_smoothing_enabled(false);
        camera.set_position_smoothing_enabled(false);
    }

    fn handle_primary_press(&mut self) {
        match self.placement_mode.as_ref() {
            Some(PlacementMode::Building(kind)) => {
                self.handle_build_click(*kind);
            }
            Some(PlacementMode::Fields { .. }) => {
                self.begin_field_drag();
            }
            None => {
                self.handle_tile_click();
            }
        }
    }

    fn handle_primary_release(&mut self) {
        if matches!(self.placement_mode, Some(PlacementMode::Fields { .. })) {
            self.finish_field_drag();
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
                self.base_mut().queue_redraw();
            }
            Err(error) => {
                godot_warn!("GameWorld: building placement rejected: {error:?}");
            }
        }
    }

    fn handle_tile_click(&mut self) {
        let mouse_pos = self.base().get_local_mouse_position();

        if let Some(coord) = Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        ) {
            let targets =
                self.with_rendered_surface_world(|world| click_selection_targets_at(world, coord));
            if targets.tile.is_none() {
                godot_error!("GameWorld: selected tile entity unavailable");
                self.disable_processing();
                return;
            }

            let events = apply_click_selection_targets(
                &mut self.selected_cell,
                &mut self.selected_npc,
                &mut self.selected_building,
                targets,
            );
            self.base_mut().queue_redraw();
            self.emit_selection_events(events);
        } else {
            self.clear_tile_selection();
            self.clear_npc_selection();
            self.clear_building_selection();
        }
    }

    fn handle_mouse_wheel(&mut self, factor: f32) {
        let mut cam = self.camera.clone();
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
            self.base_mut().queue_redraw();
            self.signals().tile_deselected().emit();
        }
    }

    fn clear_npc_selection(&mut self) {
        if self.selected_npc.take().is_some() {
            self.base_mut().queue_redraw();
            self.signals().npc_deselected().emit();
        }
    }

    fn clear_building_selection(&mut self) {
        if self.selected_building.take().is_some() {
            self.base_mut().queue_redraw();
            self.signals().building_deselected().emit();
        }
    }

    fn emit_selection_events(&mut self, events: Vec<SelectionEvent>) {
        for event in events {
            match event {
                SelectionEvent::TileSelected(entity) => {
                    let Some(tile_entity_id) = encode_entity_id(entity) else {
                        godot_error!("GameWorld: selected tile entity id is too large for Godot");
                        continue;
                    };
                    self.signals().tile_selected().emit(tile_entity_id);
                }
                SelectionEvent::TileDeselected => {
                    self.signals().tile_deselected().emit();
                }
                SelectionEvent::NpcSelected(entity) => {
                    let Some(npc_entity_id) = encode_entity_id(entity) else {
                        godot_error!("GameWorld: selected NPC entity id is too large for Godot");
                        continue;
                    };
                    self.signals().npc_selected().emit(npc_entity_id);
                }
                SelectionEvent::NpcDeselected => {
                    self.signals().npc_deselected().emit();
                }
                SelectionEvent::BuildingSelected(entity) => {
                    let Some(building_entity_id) = encode_entity_id(entity) else {
                        godot_error!(
                            "GameWorld: selected building entity id is too large for Godot"
                        );
                        continue;
                    };
                    self.signals().building_selected().emit(building_entity_id);
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

        let Some(entity_id) = encode_entity_id(target.entity) else {
            godot_error!("GameWorld: hovered map entity id is too large for Godot");
            self.hovered_map_entity = None;
            self.signals().map_entity_unhovered().emit();
            return;
        };

        self.signals()
            .map_entity_hovered()
            .emit(target.kind.signal_value(), entity_id);
    }

    fn start_build_mode(&mut self, kind: BuildingKind) {
        self.placement_mode = Some(PlacementMode::Building(kind));
        self.clear_tile_selection();
        self.clear_npc_selection();
        self.clear_building_selection();
        self.clear_hovered_map_entity();
        self.base_mut().queue_redraw();
    }

    fn start_field_placement_mode(&mut self, farm: Entity) {
        self.placement_mode = Some(PlacementMode::Fields {
            farm,
            drag_cells: Vec::new(),
        });
        self.clear_tile_selection();
        self.clear_npc_selection();
        self.clear_hovered_map_entity();
        self.base_mut().queue_redraw();
    }

    fn cancel_placement_mode(&mut self) {
        if self.placement_mode.take().is_some() {
            self.base_mut().queue_redraw();
        }
    }

    fn mark_input_handled(&self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }

    fn placement_origin_under_mouse(&self) -> Option<CellCoord> {
        let mouse_pos = self.base().get_local_mouse_position();
        Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        )
    }

    fn building_preview(&self) -> Option<(BuildingFootprint, bool)> {
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

        Some((footprint, valid))
    }

    fn field_previews(&self) -> Vec<FieldPlacementPreview> {
        let Some(PlacementMode::Fields { farm, drag_cells }) = self.placement_mode.as_ref() else {
            return Vec::new();
        };

        let coords = if drag_cells.is_empty() {
            self.placement_origin_under_mouse()
                .map(|coord| vec![coord])
                .unwrap_or_default()
        } else {
            drag_cells.clone()
        };

        self.game
            .validate_field_blueprint_placement_batch(self.rendered_surface, *farm, coords)
    }

    fn begin_field_drag(&mut self) {
        let Some(coord) = self.placement_origin_under_mouse() else {
            return;
        };
        if let Some(PlacementMode::Fields { drag_cells, .. }) = &mut self.placement_mode {
            drag_cells.clear();
            append_field_drag_cell(drag_cells, Some(coord));
            self.base_mut().queue_redraw();
        }
    }

    fn update_field_drag_current(&mut self) {
        let coord = self.placement_origin_under_mouse();
        if let Some(PlacementMode::Fields { drag_cells, .. }) = &mut self.placement_mode {
            if !drag_cells.is_empty() {
                let before_len = drag_cells.len();
                append_field_drag_cell(drag_cells, coord);
                if drag_cells.len() != before_len {
                    self.base_mut().queue_redraw();
                }
            }
        }
    }

    fn finish_field_drag(&mut self) {
        let Some(PlacementMode::Fields { farm, drag_cells }) = self.placement_mode.clone() else {
            return;
        };

        if drag_cells.is_empty() {
            self.placement_mode = Some(PlacementMode::Fields {
                farm,
                drag_cells: Vec::new(),
            });
            self.base_mut().queue_redraw();
            return;
        }

        let result = self
            .game
            .place_field_blueprints(self.rendered_surface, farm, drag_cells);
        if !result.rejected.is_empty() {
            for rejected in &result.rejected {
                godot_warn!(
                    "GameWorld: field placement rejected at ({}, {}): {:?}",
                    rejected.coord.x(),
                    rejected.coord.y(),
                    rejected.error
                );
            }
        }

        if !result.placed.is_empty() {
            self.sync_building_sprites();
        }
        self.placement_mode = Some(PlacementMode::Fields {
            farm,
            drag_cells: Vec::new(),
        });
        self.base_mut().queue_redraw();
    }

    fn map_entity_target_under_mouse(&self) -> Option<MapEntityTarget> {
        if self.placement_mode.is_some() {
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

        let local_mouse_pos = self.base().get_local_mouse_position();
        let coord = Grid::world_to_cell(
            WorldPosition::new(local_mouse_pos.x, local_mouse_pos.y),
            self.grid_size(),
        )?;

        self.with_rendered_surface_world(|world| map_entity_target_at(world, coord))
    }

    fn grid_size(&self) -> game_engine::grid::GridSize {
        self.game.grid_size(self.rendered_surface)
    }

    pub(crate) fn with_rendered_surface_world<R>(&self, f: impl FnOnce(&World) -> R) -> R {
        self.game.with_surface_world(self.rendered_surface, f)
    }

    pub(crate) fn task_table_rows(&self) -> Vec<TaskTableRow> {
        self.with_rendered_surface_world(query_task_table_rows)
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
            let modulate = building_sprite_modulate(building.state);
            if !self.building_sprites.contains_key(&building.entity) {
                let mut sprite = Sprite2D::new_alloc();
                sprite.set_texture(&texture);
                sprite.set_centered(false);
                sprite.set_texture_filter(TextureFilter::NEAREST);
                sprite.set_z_index(2);
                sprite.set_position(position);
                sprite.set_modulate(modulate);
                self.base_mut().add_child(&sprite);
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
                sprite.set_modulate(modulate);
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
                set_npc_animation(&mut sprite, npc);
                self.base_mut().add_child(&sprite);
                self.npc_sprites.insert(
                    npc.entity,
                    RenderedNpcSprite {
                        appearance: npc.appearance,
                        sprite,
                    },
                );
                continue;
            }

            if let Some(rendered) = self.npc_sprites.get_mut(&npc.entity) {
                rendered.sprite.set_position(position);
                set_npc_animation(&mut rendered.sprite, npc);
            }
        }

        if let Some(selected) = self.selected_npc {
            if let Some(coord) = selected_coord {
                if selected.coord != coord {
                    self.selected_npc = Some(SelectedNpc {
                        coord,
                        entity: selected.entity,
                    });
                    self.base_mut().queue_redraw();
                }
            } else {
                self.clear_npc_selection();
            }
        }
    }

    fn build_terrain_tile_set(&self, tile_size: i32) -> Option<Gd<TileSet>> {
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));

        for kind in TerrainKind::ALL {
            let path = terrain_asset_path(kind);
            let texture = load_texture(path, "GameWorld")?;
            let source_ts = build_single_tile_atlas_source(texture, tile_size);
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

        Some(tile_set)
    }

    fn build_resource_node_tile_set(&self, tile_size: i32) -> Option<Gd<TileSet>> {
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));

        for kind in ResourceKind::ALL {
            let path = resource_asset_path(kind);
            let texture = load_texture(path, "GameWorld")?;
            let source_ts = build_single_tile_atlas_source(texture, tile_size);
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

        Some(tile_set)
    }

    fn build_crop_tile_set(&self, tile_size: i32) -> Option<Gd<TileSet>> {
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));

        for (state, path) in crop_tile_asset_paths() {
            let texture = load_texture(path, "GameWorld")?;
            let source_ts = build_single_tile_atlas_source(texture, tile_size);
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

        Some(tile_set)
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

    fn resource_nodes(&self) -> Vec<(CellCoord, ResourceKind)> {
        self.with_rendered_surface_world(query_resource_nodes)
    }

    fn building_render_infos(&self) -> Vec<BuildingRenderInfo> {
        self.with_rendered_surface_world(query_building_render_infos)
    }

    fn crop_render_infos(&self) -> Vec<CropRenderInfo> {
        self.with_rendered_surface_world(query_crop_render_infos)
    }

    fn npc_render_infos(&self) -> Vec<NpcRenderInfo> {
        self.with_rendered_surface_world(query_npc_render_infos)
    }

    fn switch_rendered_surface(&mut self, surface: SurfaceId) {
        if self.rendered_surface == surface {
            return;
        }

        self.rendered_surface = surface;
        self.selected_cell = None;
        self.selected_npc = None;
        self.selected_building = None;
        self.hovered_map_entity = None;
        self.placement_mode = None;

        let mut tile_map = self.tile_map.clone();
        if !self.populate_tile_map(&mut tile_map) {
            self.disable_processing();
            return;
        }

        let mut resource_map = self.resource_node_map.clone();
        self.populate_resource_node_map(&mut resource_map);

        let mut crop_map = self.crop_map.clone();
        self.populate_crop_map(&mut crop_map);

        self.sync_building_sprites();
        self.sync_npc_sprites();

        let mut camera = self.camera.clone();
        self.configure_camera_for_surface(&mut camera);

        let active_surface_index = surface_index_i32(self.rendered_surface);

        self.base_mut().queue_redraw();
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

    #[func]
    pub(crate) fn start_warehouse_blueprint_placement(&mut self) {
        self.start_build_mode(BuildingKind::Warehouse);
    }

    #[func]
    pub(crate) fn start_town_hall_blueprint_placement(&mut self) {
        self.start_build_mode(BuildingKind::TownHall);
    }

    #[func]
    pub(crate) fn start_farm_blueprint_placement(&mut self) {
        self.start_build_mode(BuildingKind::Farm);
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

        self.start_field_placement_mode(selected.entity);
        true
    }

    #[func]
    pub(crate) fn cancel_building_blueprint_placement(&mut self) {
        self.cancel_placement_mode();
    }
}

fn surface_index_i32(surface: SurfaceId) -> i32 {
    i32::try_from(surface.index()).unwrap_or(i32::MAX)
}

fn encode_entity_id(entity: Entity) -> Option<i64> {
    i64::try_from(entity.to_bits()).ok()
}

pub(crate) fn decode_entity_id(entity_id: i64) -> Option<Entity> {
    let bits = u64::try_from(entity_id).ok()?;
    Entity::try_from_bits(bits)
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

fn building_entity_at(world: &World, coord: CellCoord) -> Option<Entity> {
    selected_building_at(world, coord).map(|selected| selected.entity)
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

fn query_resource_nodes(world: &World) -> Vec<(CellCoord, ResourceKind)> {
    world
        .try_query::<(&TilePosition, &ResourceNode, &Tile)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, node, _)| (position.coord, node.kind))
                .collect()
        })
        .unwrap_or_default()
}

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
                        || world.get::<AiHarvestField>(entity).is_some(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn query_task_table_rows(world: &World) -> Vec<TaskTableRow> {
    let mut rows = world
        .try_query::<(Entity, &ProgressBuildingConstruction)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter_map(|(entity, construction)| {
                    let entity_id = encode_entity_id(entity)?;
                    Some(TaskTableRow {
                        entity_id,
                        task_type: ProgressBuildingConstruction::label().to_string(),
                        details: format_construction_task_details(world, construction.blueprint()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(mut query) = world.try_query::<(Entity, &SeedField)>() {
        rows.extend(query.iter(world).filter_map(|(entity, seed)| {
            let entity_id = encode_entity_id(entity)?;
            Some(TaskTableRow {
                entity_id,
                task_type: SeedField::label().to_string(),
                details: format_field_task_details(world, "Field", seed.field()),
            })
        }));
    }

    if let Some(mut query) = world.try_query::<(Entity, &HarvestField)>() {
        rows.extend(query.iter(world).filter_map(|(entity, harvest)| {
            let entity_id = encode_entity_id(entity)?;
            Some(TaskTableRow {
                entity_id,
                task_type: HarvestField::label().to_string(),
                details: format_field_task_details(world, "Field", harvest.field()),
            })
        }));
    }

    rows.sort_by_key(|row| row.entity_id);
    rows
}

fn format_construction_task_details(world: &World, blueprint: Entity) -> String {
    let blueprint_id = encode_entity_id(blueprint)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let Some(blueprint_data) = world.get::<BuildingBlueprint>(blueprint) else {
        return format!("Blueprint {blueprint_id}: unavailable");
    };
    let Some(progress) = world.get::<ConstructionProgress>(blueprint) else {
        return format!("Blueprint {blueprint_id}: unavailable");
    };

    let origin = blueprint_data.footprint.origin();
    format!(
        "Blueprint {}: {} at ({}, {}), progress {}",
        blueprint_id,
        blueprint_data.kind.label(),
        origin.x(),
        origin.y(),
        format_deposited_over_required(
            progress.deposited(),
            blueprint_data.kind.definition().construction_cost()
        )
    )
}

fn format_field_task_details(world: &World, label: &str, field: Entity) -> String {
    let field_id = encode_entity_id(field)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let Some(building) = world.get::<Building>(field) else {
        return format!("{label} {field_id}: unavailable");
    };
    let origin = building.footprint.origin();
    format!("{label} {field_id}: at ({}, {})", origin.x(), origin.y())
}

fn format_deposited_over_required(progress: ResourceAmounts, cost: ResourceAmounts) -> String {
    let parts = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let required = cost.get(kind);
            (required > 0).then(|| format!("{}: {}/{}", kind.label(), progress.get(kind), required))
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(", ")
    }
}

fn cell_top_left(coord: CellCoord) -> Vector2 {
    Vector2::new(
        coord.x() as f32 * grid::TILE_SIZE,
        coord.y() as f32 * grid::TILE_SIZE,
    )
}

fn npc_top_left(coord: CellCoord, subtile_offset: SubtileOffset) -> Vector2 {
    cell_top_left(coord)
        + Vector2::new(
            subtile_units_to_pixels(subtile_offset.x_units),
            subtile_units_to_pixels(subtile_offset.y_units),
        )
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

fn npc_animation_name(npc: NpcRenderInfo) -> &'static str {
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

fn building_asset_path(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Warehouse => BUILDING_WAREHOUSE_PATH,
        BuildingKind::TownHall => BUILDING_TOWNHALL_PATH,
        BuildingKind::Farm => BUILDING_FARM_PATH,
        BuildingKind::Field => BUILDING_FIELD_PATH,
    }
}

fn terrain_source_id(kind: TerrainKind) -> i32 {
    kind as i32
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

fn append_field_drag_cell(drag_cells: &mut Vec<CellCoord>, coord: Option<CellCoord>) {
    let Some(coord) = coord else {
        return;
    };
    if drag_cells.last().copied() != Some(coord) {
        drag_cells.push(coord);
    }
}

fn build_single_tile_atlas_source(texture: Gd<Texture2D>, tile_size: i32) -> Gd<TileSetSource> {
    let v2 = |x: i32, y: i32| Vector2i::new(x, y);
    let mut source = TileSetAtlasSource::new_gd();
    source.set_texture(&texture);
    source.set_texture_region_size(v2(tile_size, tile_size));
    source.create_tile_ex(v2(0, 0)).done();
    source.upcast::<TileSetSource>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::buildings::BuildingBlueprintBundle;
    use game_engine::farming::{FarmInventory, FieldOwner};
    use game_engine::grid::GridSize;
    use game_engine::npcs::InitialNpcBundle;
    use game_engine::tasks::{ProgressBuildingConstructionTaskBundle, Task};
    use game_engine::tile::TileBundle;

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
    fn task_table_rows_format_construction_tasks() {
        let mut world = World::new();
        let blueprint = world
            .spawn(BuildingBlueprintBundle::new(
                BuildingKind::Warehouse,
                BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
            ))
            .id();
        let task = world
            .spawn(ProgressBuildingConstructionTaskBundle::new(blueprint))
            .id();

        let rows = query_task_table_rows(&world);

        assert_eq!(
            rows,
            vec![TaskTableRow {
                entity_id: encode_entity_id(task).expect("task entity id should encode"),
                task_type: "ProgressBuildingConstruction".to_string(),
                details: format!(
                    "Blueprint {}: Warehouse at (4, 7), progress Wood: 0/40, Stone: 0/20",
                    encode_entity_id(blueprint).expect("blueprint entity id should encode")
                ),
            }]
        );
    }

    #[test]
    fn task_table_rows_format_farming_tasks() {
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
        let field = world
            .spawn((
                Building::new(
                    BuildingKind::Field,
                    BuildingFootprint::new(CellCoord::new(3, 1), 1, 1),
                ),
                FieldOwner::new(farm),
                FieldCrop::seedable(),
            ))
            .id();
        let seed_task = world.spawn((Task, SeedField::new(field))).id();
        let harvest_task = world.spawn((Task, HarvestField::new(field))).id();

        let rows = query_task_table_rows(&world);

        assert_eq!(rows.len(), 2);
        assert!(rows.contains(&TaskTableRow {
            entity_id: encode_entity_id(seed_task).expect("task entity id should encode"),
            task_type: "SeedField".to_string(),
            details: format!(
                "Field {}: at (3, 1)",
                encode_entity_id(field).expect("field entity id should encode")
            ),
        }));
        assert!(rows.contains(&TaskTableRow {
            entity_id: encode_entity_id(harvest_task).expect("task entity id should encode"),
            task_type: "HarvestField".to_string(),
            details: format!(
                "Field {}: at (3, 1)",
                encode_entity_id(field).expect("field entity id should encode")
            ),
        }));
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
    fn append_field_drag_cell_records_path_and_skips_same_cell_samples() {
        let mut cells = Vec::new();

        append_field_drag_cell(&mut cells, Some(CellCoord::new(3, 2)));
        append_field_drag_cell(&mut cells, Some(CellCoord::new(3, 2)));
        append_field_drag_cell(&mut cells, None);
        append_field_drag_cell(&mut cells, Some(CellCoord::new(4, 2)));

        assert_eq!(cells, vec![CellCoord::new(3, 2), CellCoord::new(4, 2)]);
    }

    #[test]
    fn building_asset_paths_include_farming_assets() {
        assert_eq!(
            building_asset_path(BuildingKind::Farm),
            "res://assets/generated/building_farm.png"
        );
        assert_eq!(
            building_asset_path(BuildingKind::Field),
            "res://assets/generated/building_field.png"
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

    fn indexed_tile_entity(world: &World, coord: CellCoord) -> Entity {
        world
            .resource::<TileIndex>()
            .get(coord)
            .expect("test tile should exist in index")
    }
}
