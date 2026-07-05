use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
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
    Node2D, ResourceLoader, Texture2D, TileMapLayer, TileSet, TileSetAtlasSource, TileSetSource,
};
use godot::global::MouseButton;
use godot::obj::{OnEditor, Singleton};
use godot::prelude::*;

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
const TERRAIN_GRASS_PATH: &str = "res://assets/generated/terrain_grass.png";
const RESOURCE_WOOD_PATH: &str = "res://assets/generated/resource_wood.png";
const RESOURCE_STONE_PATH: &str = "res://assets/generated/resource_stone.png";
const RESOURCE_FOOD_PATH: &str = "res://assets/generated/resource_food.png";
const RESOURCE_GOLD_PATH: &str = "res://assets/generated/resource_gold.png";
const NPC_COLONIST_PATH: &str = "res://assets/generated/npc_colonist.png";

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

#[derive(GodotClass)]
#[class(base = Node2D)]
pub(crate) struct GameWorld {
    #[export]
    tile_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    resource_node_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    npc_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    camera: OnEditor<Gd<Camera2D>>,

    game: GameSimulation,
    rendered_surface: SurfaceId,
    selected_cell: Option<SelectedCell>,
    selected_npc: Option<SelectedNpc>,
    tile_source_id: Option<i32>,
    _tile_set: Option<Gd<TileSet>>,
    _resource_node_tile_set: Option<Gd<TileSet>>,
    _npc_tile_set: Option<Gd<TileSet>>,

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
            npc_map: OnEditor::default(),
            camera: OnEditor::default(),
            game,
            rendered_surface,
            selected_cell: None,
            selected_npc: None,
            tile_source_id: None,
            _tile_set: None,
            _resource_node_tile_set: None,
            _npc_tile_set: None,
            base,
        }
    }

    fn ready(&mut self) {
        let mut tm = self.tile_map.clone();
        let mut resource_map = self.resource_node_map.clone();
        let mut npc_map = self.npc_map.clone();
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

        let Some(npc_tile_set) = self.build_npc_tile_set(ts) else {
            self.disable_processing();
            return;
        };
        npc_map.set_tile_set(&npc_tile_set);
        self._npc_tile_set = Some(npc_tile_set);
        npc_map.set_navigation_enabled(false);
        npc_map.set_texture_filter(TextureFilter::NEAREST);
        npc_map.set_z_index(2);
        self.populate_npc_map(&mut npc_map);

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
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }

        match mouse.get_button_index() {
            MouseButton::LEFT => {
                self.handle_tile_click();
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
        let Some(coords) = self.game.tile_coords(self.rendered_surface) else {
            godot_error!("GameWorld: rendered surface tile coordinates unavailable");
            return false;
        };

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

    fn handle_tile_click(&mut self) {
        let mouse_pos = self.base().get_local_mouse_position();

        if let Some(coord) = Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        ) {
            if let Some(entity) = self.npc_entity_at(coord) {
                self.select_npc(coord, entity);
                return;
            }

            self.select_tile(coord);
        } else {
            self.clear_tile_selection();
            self.clear_npc_selection();
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
        self.selected_cell = Some(SelectedCell { coord, entity });
        self.base_mut().queue_redraw();
        self.signals().tile_selected().emit(tile_entity_id);
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

    fn grid_size(&self) -> game_engine::grid::GridSize {
        self.game
            .grid_size(self.rendered_surface)
            .expect("rendered surface should exist")
    }

    pub(crate) fn with_rendered_surface_world<R>(&self, f: impl FnOnce(&World) -> R) -> Option<R> {
        self.game.with_surface_world(self.rendered_surface, f)
    }

    fn tile_entity_at(&self, coord: CellCoord) -> Option<Entity> {
        self.with_rendered_surface_world(|world| {
            let index = world.resource::<TileIndex>();
            let entity = index.get(coord)?;
            world.get::<Tile>(entity)?;
            Some(entity)
        })
        .flatten()
    }

    fn npc_entity_at(&self, coord: CellCoord) -> Option<Entity> {
        self.with_rendered_surface_world(|world| {
            let mut query = world.try_query::<(Entity, &NpcPosition, &Npc)>()?;
            query
                .iter(world)
                .find_map(|(entity, position, _)| (position.coord == coord).then_some(entity))
        })
        .flatten()
    }

    fn build_terrain_tile_set(&self, tile_size: i32) -> Option<(Gd<TileSet>, i32)> {
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));

        let texture = load_texture(TERRAIN_GRASS_PATH)?;
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
            let texture = load_texture(path)?;
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

    fn build_npc_tile_set(&self, tile_size: i32) -> Option<Gd<TileSet>> {
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));

        let texture = load_texture(NPC_COLONIST_PATH)?;
        let source_ts = build_single_tile_atlas_source(texture, tile_size);
        let source_id = tile_set.add_source(&source_ts);
        if source_id < 0 {
            godot_error!("GameWorld: failed to add NPC tile atlas source");
            return None;
        }

        Some(tile_set)
    }

    fn populate_resource_node_map(&mut self, resource_map: &mut Gd<TileMapLayer>) {
        resource_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let Some(nodes) = self.resource_nodes() else {
            godot_error!("GameWorld: rendered surface no longer exists");
            self.disable_processing();
            return;
        };

        for (coord, kind) in nodes {
            resource_map
                .set_cell_ex(v2(coord.x(), coord.y()))
                .source_id(kind as i32)
                .atlas_coords(v2(0, 0))
                .done();
        }
        resource_map.update_internals();
    }

    fn resource_nodes(&self) -> Option<Vec<(CellCoord, ResourceKind)>> {
        self.with_rendered_surface_world(query_resource_nodes)
    }

    fn populate_npc_map(&mut self, npc_map: &mut Gd<TileMapLayer>) {
        npc_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let Some(npcs) = self.npc_positions() else {
            godot_error!("GameWorld: rendered surface no longer exists");
            self.disable_processing();
            return;
        };

        for coord in npcs {
            npc_map
                .set_cell_ex(v2(coord.x(), coord.y()))
                .source_id(0)
                .atlas_coords(v2(0, 0))
                .done();
        }
        npc_map.update_internals();
    }

    fn npc_positions(&self) -> Option<Vec<CellCoord>> {
        self.with_rendered_surface_world(query_npc_positions)
    }

    fn switch_rendered_surface(&mut self, surface: SurfaceId) {
        if self.rendered_surface == surface {
            return;
        }

        self.rendered_surface = surface;
        self.selected_cell = None;
        self.selected_npc = None;

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

        let mut npc_map = self.npc_map.clone();
        self.populate_npc_map(&mut npc_map);

        let mut camera = self.camera.clone();
        self.configure_camera_for_surface(&mut camera);

        let active_surface_index = surface_index_i32(self.rendered_surface);

        self.base_mut().queue_redraw();
        self.signals().tile_deselected().emit();
        self.signals().npc_deselected().emit();
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
        let Some(surface) = self.game.surface_id_at(index) else {
            godot_warn!("GameWorld: ignoring unknown surface index {index}");
            return false;
        };

        self.switch_rendered_surface(surface);
        true
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

fn query_npc_positions(world: &World) -> Vec<CellCoord> {
    world
        .try_query::<(&NpcPosition, &Npc)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(position, _)| position.coord)
                .collect()
        })
        .unwrap_or_default()
}

fn resource_asset_path(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => RESOURCE_WOOD_PATH,
        ResourceKind::Stone => RESOURCE_STONE_PATH,
        ResourceKind::Food => RESOURCE_FOOD_PATH,
        ResourceKind::Gold => RESOURCE_GOLD_PATH,
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

fn load_texture(path: &str) -> Option<Gd<Texture2D>> {
    let Some(resource) = ResourceLoader::singleton()
        .load_ex(path)
        .type_hint("Texture2D")
        .done()
    else {
        godot_error!("GameWorld: failed to load texture asset {path}");
        return None;
    };

    match resource.try_cast::<Texture2D>() {
        Ok(texture) => Some(texture),
        Err(resource) => {
            godot_error!(
                "GameWorld: loaded asset {path} as {}, expected Texture2D",
                resource.get_class()
            );
            None
        }
    }
}
