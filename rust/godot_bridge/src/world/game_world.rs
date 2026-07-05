use crate::assets::{load_texture, resource_asset_path};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingBlueprintKind, BuildingFootprint,
};
use game_engine::components::{Tile, TilePosition};
use game_engine::grid::{self, CellCoord, Grid, WorldPosition};
use game_engine::npcs::{Npc, NpcPosition};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::ResourceKind;
use game_engine::simulation::{GameSimulation, SurfaceId};
use game_engine::tile::TileIndex;
use godot::builtin::Side;
use godot::classes::{
    canvas_item::TextureFilter, Camera2D, INode2D, Input, InputEvent, InputEventMouseButton,
    Node2D, Sprite2D, Texture2D, TileMapLayer, TileSet, TileSetAtlasSource, TileSetSource,
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
const TERRAIN_GRASS_PATH: &str = "res://assets/generated/terrain_grass.png";
const NPC_COLONIST_PATH: &str = "res://assets/generated/npc_colonist.png";
const BUILDING_WAREHOUSE_PATH: &str = "res://assets/generated/building_warehouse.png";
const BUILDING_TOWNHALL_PATH: &str = "res://assets/generated/building_townhall.png";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NpcRenderInfo {
    entity: Entity,
    coord: CellCoord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildingRenderInfo {
    entity: Entity,
    kind: BuildingBlueprintKind,
    footprint: BuildingFootprint,
    is_blueprint: bool,
}

#[derive(GodotClass)]
#[class(base = Node2D)]
pub(crate) struct GameWorld {
    #[export]
    tile_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    resource_node_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    camera: OnEditor<Gd<Camera2D>>,

    game: GameSimulation,
    rendered_surface: SurfaceId,
    selected_cell: Option<SelectedCell>,
    selected_npc: Option<SelectedNpc>,
    selected_building: Option<SelectedBuilding>,
    build_mode: Option<BuildingBlueprintKind>,
    tile_source_id: Option<i32>,
    _tile_set: Option<Gd<TileSet>>,
    _resource_node_tile_set: Option<Gd<TileSet>>,
    npc_texture: Option<Gd<Texture2D>>,
    npc_sprites: HashMap<Entity, Gd<Sprite2D>>,
    building_textures: HashMap<BuildingBlueprintKind, Gd<Texture2D>>,
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
            camera: OnEditor::default(),
            game,
            rendered_surface,
            selected_cell: None,
            selected_npc: None,
            selected_building: None,
            build_mode: None,
            tile_source_id: None,
            _tile_set: None,
            _resource_node_tile_set: None,
            npc_texture: None,
            npc_sprites: HashMap::new(),
            building_textures: HashMap::new(),
            building_sprites: HashMap::new(),
            base,
        }
    }

    fn ready(&mut self) {
        let mut tm = self.tile_map.clone();
        let mut resource_map = self.resource_node_map.clone();
        let mut cam = self.camera.clone();

        let ts = grid::TILE_SIZE as i32;
        let Some((tile_set, source_id)) = self.build_terrain_tile_set(ts) else {
            self.disable_processing();
            return;
        };

        self.tile_source_id = Some(source_id);
        tm.set_tile_set(&tile_set);
        self._tile_set = Some(tile_set);
        tm.set_navigation_enabled(false);
        tm.set_texture_filter(TextureFilter::NEAREST);
        tm.set_draw_behind_parent(true);

        if !self.populate_tile_map(&mut tm, source_id) {
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

        let Some(npc_texture) = load_texture(NPC_COLONIST_PATH, "GameWorld") else {
            self.disable_processing();
            return;
        };
        self.npc_texture = Some(npc_texture);
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

        self.game.tick(delta as f32);
        self.sync_npc_sprites();
        if self.build_mode.is_some() {
            self.base_mut().queue_redraw();
        }
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
        let build_preview = self.build_preview();
        let blueprint_footprints = self
            .building_render_infos()
            .into_iter()
            .filter_map(|building| building.is_blueprint.then_some(building.footprint))
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

        if let Some((footprint, valid)) = build_preview {
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
        if event.is_action_pressed(ACTION_MENU_TOGGLE) && self.build_mode.is_some() {
            self.cancel_build_mode();
            self.mark_input_handled();
            return;
        }

        let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }

        match mouse.get_button_index() {
            MouseButton::LEFT => {
                self.handle_primary_click();
            }
            MouseButton::RIGHT => {
                if self.build_mode.is_some() {
                    self.cancel_build_mode();
                    self.mark_input_handled();
                }
            }
            MouseButton::WHEEL_UP => {
                self.handle_mouse_wheel(ZOOM_FACTOR);
            }
            MouseButton::WHEEL_DOWN => {
                self.handle_mouse_wheel(1.0 / ZOOM_FACTOR);
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

    fn populate_tile_map(&self, tile_map: &mut Gd<TileMapLayer>, source_id: i32) -> bool {
        tile_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let coords = self.game.tile_coords(self.rendered_surface);

        for coord in coords {
            tile_map
                .set_cell_ex(v2(coord.x(), coord.y()))
                .source_id(source_id)
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

    fn handle_primary_click(&mut self) {
        if let Some(kind) = self.build_mode {
            self.handle_build_click(kind);
            return;
        }

        self.handle_tile_click();
    }

    fn handle_build_click(&mut self, kind: BuildingBlueprintKind) {
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
            if let Some(entity) = self.building_entity_at(coord) {
                self.select_building(entity);
                return;
            }

            if let Some(entity) = self.npc_entity_at(coord) {
                self.select_npc(coord, entity);
                return;
            }

            self.select_tile(coord);
        } else {
            self.clear_tile_selection();
            self.clear_npc_selection();
            self.clear_building_selection();
        }
    }

    fn select_tile(&mut self, coord: CellCoord) {
        let Some(entity) = self.tile_entity_at(coord) else {
            godot_error!("GameWorld: selected tile entity unavailable");
            self.disable_processing();
            return;
        };

        if self.selected_cell.map(|selected| selected.entity) == Some(entity) {
            self.clear_tile_selection();
            return;
        }

        let Some(tile_entity_id) = encode_entity_id(entity) else {
            godot_error!("GameWorld: selected tile entity id is too large for Godot");
            return;
        };

        self.clear_npc_selection();
        self.clear_building_selection();
        self.selected_cell = Some(SelectedCell { coord, entity });
        self.base_mut().queue_redraw();
        self.signals().tile_selected().emit(tile_entity_id);
    }

    fn select_building(&mut self, entity: Entity) {
        if self.selected_building.map(|selected| selected.entity) == Some(entity) {
            self.clear_building_selection();
            return;
        }

        let Some(footprint) = self.building_footprint(entity) else {
            godot_error!("GameWorld: selected building entity unavailable");
            return;
        };
        let Some(building_entity_id) = encode_entity_id(entity) else {
            godot_error!("GameWorld: selected building entity id is too large for Godot");
            return;
        };

        self.clear_tile_selection();
        self.clear_npc_selection();
        self.selected_building = Some(SelectedBuilding { footprint, entity });
        self.base_mut().queue_redraw();
        self.signals().building_selected().emit(building_entity_id);
    }

    fn select_npc(&mut self, coord: CellCoord, entity: Entity) {
        if self.selected_npc.map(|selected| selected.entity) == Some(entity) {
            self.clear_npc_selection();
            return;
        }

        let Some(npc_entity_id) = encode_entity_id(entity) else {
            godot_error!("GameWorld: selected NPC entity id is too large for Godot");
            return;
        };

        self.clear_tile_selection();
        self.clear_building_selection();
        self.selected_npc = Some(SelectedNpc { coord, entity });
        self.base_mut().queue_redraw();
        self.signals().npc_selected().emit(npc_entity_id);
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

    fn start_build_mode(&mut self, kind: BuildingBlueprintKind) {
        self.build_mode = Some(kind);
        self.clear_tile_selection();
        self.clear_npc_selection();
        self.clear_building_selection();
        self.base_mut().queue_redraw();
    }

    fn cancel_build_mode(&mut self) {
        if self.build_mode.take().is_some() {
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

    fn build_preview(&self) -> Option<(BuildingFootprint, bool)> {
        let kind = self.build_mode?;
        let origin = self.placement_origin_under_mouse()?;
        let definition = kind.definition();
        let footprint = BuildingFootprint::new(origin, definition.width(), definition.height());
        let valid = self
            .game
            .validate_building_blueprint_placement(self.rendered_surface, kind, origin)
            .is_ok();

        Some((footprint, valid))
    }

    fn grid_size(&self) -> game_engine::grid::GridSize {
        self.game.grid_size(self.rendered_surface)
    }

    pub(crate) fn with_rendered_surface_world<R>(&self, f: impl FnOnce(&World) -> R) -> R {
        self.game.with_surface_world(self.rendered_surface, f)
    }

    fn tile_entity_at(&self, coord: CellCoord) -> Option<Entity> {
        self.with_rendered_surface_world(|world| {
            let index = world.resource::<TileIndex>();
            let entity = index.get(coord)?;
            world.get::<Tile>(entity)?;
            Some(entity)
        })
    }

    fn npc_entity_at(&self, coord: CellCoord) -> Option<Entity> {
        self.with_rendered_surface_world(|world| {
            let mut query = world.try_query::<(Entity, &NpcPosition, &Npc)>()?;
            query
                .iter(world)
                .find_map(|(entity, position, _)| (position.coord == coord).then_some(entity))
        })
    }

    fn building_entity_at(&self, coord: CellCoord) -> Option<Entity> {
        self.with_rendered_surface_world(|world| {
            let mut query = world.try_query::<(Entity, &BuildingFootprint, &Building)>()?;
            query
                .iter(world)
                .find_map(|(entity, footprint, _)| footprint.contains(coord).then_some(entity))
        })
    }

    fn building_footprint(&self, entity: Entity) -> Option<BuildingFootprint> {
        self.with_rendered_surface_world(|world| {
            world.get::<Building>(entity)?;
            world.get::<BuildingFootprint>(entity).copied()
        })
    }

    fn sync_building_sprites(&mut self) {
        let buildings = self.building_render_infos();

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
            }
        }

        for building in buildings {
            let Some(texture) = self.building_texture(building.kind) else {
                godot_error!(
                    "GameWorld: building texture missing for {:?}",
                    building.kind
                );
                self.disable_processing();
                return;
            };

            let position = cell_top_left(building.footprint.origin());
            if !self.building_sprites.contains_key(&building.entity) {
                let mut sprite = Sprite2D::new_alloc();
                sprite.set_texture(&texture);
                sprite.set_centered(false);
                sprite.set_texture_filter(TextureFilter::NEAREST);
                sprite.set_z_index(2);
                sprite.set_position(position);
                sprite.set_modulate(building_sprite_modulate(building.is_blueprint));
                self.base_mut().add_child(&sprite);
                self.building_sprites.insert(building.entity, sprite);
                continue;
            }

            if let Some(sprite) = self.building_sprites.get_mut(&building.entity) {
                sprite.set_texture(&texture);
                sprite.set_position(position);
                sprite.set_modulate(building_sprite_modulate(building.is_blueprint));
            }
        }

        if let Some(selected) = self.selected_building {
            let selected_still_exists = active_entities.contains(&selected.entity);
            if !selected_still_exists {
                self.clear_building_selection();
            }
        }
    }

    fn sync_npc_sprites(&mut self) {
        let npcs = self.npc_render_infos();
        let Some(texture) = self.npc_texture.clone() else {
            godot_error!("GameWorld: NPC texture not initialized");
            self.disable_processing();
            return;
        };

        let active_entities: HashSet<Entity> = npcs.iter().map(|npc| npc.entity).collect();
        let stale_entities: Vec<Entity> = self
            .npc_sprites
            .keys()
            .copied()
            .filter(|entity| !active_entities.contains(entity))
            .collect();
        for entity in stale_entities {
            if let Some(mut sprite) = self.npc_sprites.remove(&entity) {
                sprite.queue_free();
            }
        }

        let selected_entity = self.selected_npc.map(|selected| selected.entity);
        let mut selected_coord = None;
        for npc in npcs {
            if selected_entity == Some(npc.entity) {
                selected_coord = Some(npc.coord);
            }

            let position = cell_top_left(npc.coord);
            if !self.npc_sprites.contains_key(&npc.entity) {
                let mut sprite = Sprite2D::new_alloc();
                sprite.set_texture(&texture);
                sprite.set_centered(false);
                sprite.set_texture_filter(TextureFilter::NEAREST);
                sprite.set_z_index(3);
                sprite.set_position(position);
                self.base_mut().add_child(&sprite);
                self.npc_sprites.insert(npc.entity, sprite);
                continue;
            }

            if let Some(sprite) = self.npc_sprites.get_mut(&npc.entity) {
                sprite.set_position(position);
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

    fn build_terrain_tile_set(&self, tile_size: i32) -> Option<(Gd<TileSet>, i32)> {
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));

        let texture = load_texture(TERRAIN_GRASS_PATH, "GameWorld")?;
        let source_ts = build_single_tile_atlas_source(texture, tile_size);
        let source_id = tile_set.add_source(&source_ts);
        if source_id < 0 {
            godot_error!("GameWorld: failed to add terrain tile atlas source");
            return None;
        }

        Some((tile_set, source_id))
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

    fn load_building_textures(&mut self) -> bool {
        self.building_textures.clear();

        for kind in BuildingBlueprintKind::ALL {
            let path = building_asset_path(kind);
            let Some(texture) = load_texture(path, "GameWorld") else {
                return false;
            };
            self.building_textures.insert(kind, texture);
        }

        true
    }

    fn building_texture(&self, kind: BuildingBlueprintKind) -> Option<Gd<Texture2D>> {
        self.building_textures.get(&kind).cloned()
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

    fn resource_nodes(&self) -> Vec<(CellCoord, ResourceKind)> {
        self.with_rendered_surface_world(query_resource_nodes)
    }

    fn building_render_infos(&self) -> Vec<BuildingRenderInfo> {
        self.with_rendered_surface_world(query_building_render_infos)
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
        self.build_mode = None;

        let mut tile_map = self.tile_map.clone();
        let Some(tile_source_id) = self.tile_source_id else {
            godot_error!("GameWorld: tile source id not initialized");
            self.disable_processing();
            return;
        };
        if !self.populate_tile_map(&mut tile_map, tile_source_id) {
            self.disable_processing();
            return;
        }

        let mut resource_map = self.resource_node_map.clone();
        self.populate_resource_node_map(&mut resource_map);

        self.sync_building_sprites();
        self.sync_npc_sprites();

        let mut camera = self.camera.clone();
        self.configure_camera_for_surface(&mut camera);

        let active_surface_index = surface_index_i32(self.rendered_surface);

        self.base_mut().queue_redraw();
        self.signals().tile_deselected().emit();
        self.signals().npc_deselected().emit();
        self.signals().building_deselected().emit();
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
        self.start_build_mode(BuildingBlueprintKind::Warehouse);
    }

    #[func]
    pub(crate) fn start_town_hall_blueprint_placement(&mut self) {
        self.start_build_mode(BuildingBlueprintKind::TownHall);
    }

    #[func]
    pub(crate) fn cancel_building_blueprint_placement(&mut self) {
        self.cancel_build_mode();
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
    world
        .try_query::<(
            Entity,
            &Building,
            &BuildingFootprint,
            Option<&BuildingBlueprint>,
        )>()
        .map(|mut query| {
            query
                .iter(world)
                .map(
                    |(entity, building, footprint, blueprint)| BuildingRenderInfo {
                        entity,
                        kind: building.kind,
                        footprint: *footprint,
                        is_blueprint: blueprint.is_some(),
                    },
                )
                .collect()
        })
        .unwrap_or_default()
}

fn query_npc_render_infos(world: &World) -> Vec<NpcRenderInfo> {
    world
        .try_query::<(Entity, &NpcPosition, &Npc)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, position, _)| NpcRenderInfo {
                    entity,
                    coord: position.coord,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cell_top_left(coord: CellCoord) -> Vector2 {
    Vector2::new(
        coord.x() as f32 * grid::TILE_SIZE,
        coord.y() as f32 * grid::TILE_SIZE,
    )
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

fn building_sprite_modulate(is_blueprint: bool) -> Color {
    if !is_blueprint {
        return Color::from_rgb(1.0, 1.0, 1.0);
    }

    let mut color = Color::from_rgb(0.55, 0.9, 1.0);
    color.a = 0.62;
    color
}

fn building_asset_path(kind: BuildingBlueprintKind) -> &'static str {
    match kind {
        BuildingBlueprintKind::Warehouse => BUILDING_WAREHOUSE_PATH,
        BuildingBlueprintKind::TownHall => BUILDING_TOWNHALL_PATH,
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
