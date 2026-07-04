use bevy_ecs::world::World;
use game_engine::grid::{self, CellCoord, CellType, Grid, WorldPosition};
use game_engine::resource_nodes::{ResourceNode, TilePosition};
use game_engine::resources::{ResourceKind, ResourceSnapshot};
use game_engine::simulation::{GameSimulation, SurfaceId};
use godot::builtin::Side;
use godot::classes::{
    canvas_item::TextureFilter, image, Camera2D, INode2D, Image, ImageTexture, Input, InputEvent,
    InputEventMouseButton, Node2D, TileMapLayer, TileSet, TileSetAtlasSource, TileSetSource,
};
use godot::global::{Key, MouseButton};
use godot::obj::OnEditor;
use godot::prelude::*;

const ZOOM_ABSOLUTE_FLOOR: f32 = 0.001;
const ZOOM_MARGIN: f32 = 0.95;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_FACTOR: f32 = 1.1;
const PAN_SPEED: f32 = 600.0;

fn world_limit(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }

    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedCell {
    coord: CellCoord,
    cell_type: CellType,
    resource_kind: Option<ResourceKind>,
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
    tile_source_id: Option<i32>,
    _tile_set: Option<Gd<TileSet>>,
    _resource_node_tile_set: Option<Gd<TileSet>>,

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
            tile_source_id: None,
            _tile_set: None,
            _resource_node_tile_set: None,
            base,
        }
    }

    fn ready(&mut self) {
        let Some(mut tm) = self.tile_map_node() else {
            godot_error!("GameWorld: tile_map reference not set");
            self.disable_processing();
            return;
        };
        let Some(mut resource_map) = self.resource_node_map_node() else {
            godot_error!("GameWorld: resource_node_map reference not set");
            self.disable_processing();
            return;
        };
        let Some(mut cam) = self.camera_node() else {
            godot_error!("GameWorld: camera reference not set");
            self.disable_processing();
            return;
        };

        let ts = grid::TILE_SIZE as i32;
        let atlas_w = ts * 2;
        let atlas_h = ts;

        let Some(mut image) = Image::create(atlas_w, atlas_h, false, image::Format::RGBA8) else {
            godot_error!("GameWorld: failed to create tile atlas image");
            self.disable_processing();
            return;
        };
        image.fill(Color::from_rgba8(0, 0, 0, 0));

        let grass = Color::from_rgb(0.25, 0.55, 0.15);
        let brown = Color::from_rgb(0.55, 0.45, 0.25);

        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let rect = |x: i32, y: i32, w: i32, h: i32| Rect2i::new(v2(x, y), v2(w, h));

        image.fill_rect(rect(0, 0, ts, ts), grass);

        let bx = ts;
        image.fill_rect(rect(bx, 0, ts, ts), brown);

        let Some(texture) = ImageTexture::create_from_image(&image) else {
            godot_error!("GameWorld: failed to create tile atlas texture");
            self.disable_processing();
            return;
        };

        let mut source = TileSetAtlasSource::new_gd();
        source.set_texture(&texture);
        source.set_texture_region_size(v2(ts, ts));
        source.create_tile_ex(v2(0, 0)).done();
        source.create_tile_ex(v2(1, 0)).done();

        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(ts, ts));
        let source_ts = source.upcast::<TileSetSource>();
        let source_id = tile_set.add_source(&source_ts);
        if source_id < 0 {
            godot_error!("GameWorld: failed to add tile atlas source");
            self.disable_processing();
            return;
        }

        self.tile_source_id = Some(source_id);
        tm.set_tile_set(&tile_set);
        self._tile_set = Some(tile_set);
        tm.set_navigation_enabled(false);
        tm.set_texture_filter(TextureFilter::NEAREST);
        tm.set_draw_behind_parent(true);

        self.populate_tile_map(&mut tm, source_id);

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
        let Some(mut cam) = self.camera_node() else {
            return;
        };

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
        if input.is_key_pressed(Key::W) {
            dir.y -= 1.0;
        }
        if input.is_key_pressed(Key::S) {
            dir.y += 1.0;
        }
        if input.is_key_pressed(Key::A) {
            dir.x -= 1.0;
        }
        if input.is_key_pressed(Key::D) {
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
            let highlight = match selected.cell_type {
                CellType::Empty => Color::from_rgb(1.0, 0.84, 0.0),
                CellType::Building => Color::from_rgb(0.45, 0.75, 1.0),
            };
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
    fn tile_map_node(&self) -> Option<Gd<TileMapLayer>> {
        let tile_map = self.tile_map.clone();
        tile_map.is_instance_valid().then_some(tile_map)
    }

    fn resource_node_map_node(&self) -> Option<Gd<TileMapLayer>> {
        let resource_node_map = self.resource_node_map.clone();
        resource_node_map
            .is_instance_valid()
            .then_some(resource_node_map)
    }

    fn camera_node(&self) -> Option<Gd<Camera2D>> {
        let camera = self.camera.clone();
        camera.is_instance_valid().then_some(camera)
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

    fn populate_tile_map(&self, tile_map: &mut Gd<TileMapLayer>, source_id: i32) {
        tile_map.clear();
        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        for coord in self.grid_size().iter_coords() {
            let atlas_x = match self
                .game
                .cell_type(self.rendered_surface, coord)
                .unwrap_or_default()
            {
                CellType::Empty => 0,
                CellType::Building => 1,
            };
            tile_map
                .set_cell_ex(v2(coord.x(), coord.y()))
                .source_id(source_id)
                .atlas_coords(v2(atlas_x, 0))
                .done();
        }
        tile_map.update_internals();
    }

    fn configure_camera_for_surface(&self, camera: &mut Gd<Camera2D>) {
        let world_size = self.world_size();
        camera.set_position(world_size / 2.0);
        camera.set_limit(Side::LEFT, 0);
        camera.set_limit(Side::TOP, 0);
        camera.set_limit(Side::RIGHT, world_limit(world_size.x));
        camera.set_limit(Side::BOTTOM, world_limit(world_size.y));
        camera.set_limit_smoothing_enabled(false);
        camera.set_position_smoothing_enabled(false);
    }

    fn handle_tile_click(&mut self) {
        let mouse_pos = self.base().get_local_mouse_position();

        if let Some(coord) = Grid::world_to_cell(
            WorldPosition::new(mouse_pos.x, mouse_pos.y),
            self.grid_size(),
        ) {
            if self.selected_cell.map(|selected| selected.coord) == Some(coord) {
                self.clear_selection();
            } else {
                let cell_type = self
                    .game
                    .cell_type(self.rendered_surface, coord)
                    .unwrap_or_default();
                let resource_kind = self.resource_node_at(coord);
                self.selected_cell = Some(SelectedCell {
                    coord,
                    cell_type,
                    resource_kind,
                });
                self.base_mut().queue_redraw();
                let type_name = GString::from(cell_type.type_name());
                let resource_name =
                    GString::from(resource_kind.map(ResourceKind::label).unwrap_or(""));
                self.signals().tile_selected().emit(
                    coord.x(),
                    coord.y(),
                    &type_name,
                    &resource_name,
                );
            }
        } else {
            self.clear_selection();
        }
    }

    fn handle_mouse_wheel(&mut self, factor: f32) {
        let Some(mut cam) = self.camera_node() else {
            return;
        };
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

    fn clear_selection(&mut self) {
        self.selected_cell = None;
        self.base_mut().queue_redraw();
        self.signals().tile_deselected().emit();
    }

    fn grid_size(&self) -> game_engine::grid::GridSize {
        self.game
            .grid_size(self.rendered_surface)
            .expect("rendered surface should exist")
    }

    pub(crate) fn resource_snapshot(&self) -> ResourceSnapshot {
        self.game
            .resource_snapshot(self.rendered_surface)
            .expect("rendered surface should exist")
    }

    fn resource_amount(&self, kind: ResourceKind) -> u32 {
        self.game
            .resource_amount(self.rendered_surface, kind)
            .expect("rendered surface should exist")
    }

    fn add_resource(&mut self, kind: ResourceKind, amount: u32) {
        match self.game.add_resource(self.rendered_surface, kind, amount) {
            Some(true) => {
                self.signals().resources_changed().emit();
            }
            Some(false) => {
                godot_warn!(
                    "GameWorld: ignoring {} addition of {} because it would overflow u32",
                    kind.label(),
                    amount
                );
            }
            None => {
                godot_error!("GameWorld: rendered surface no longer exists");
                self.disable_processing();
            }
        }
    }

    fn build_resource_node_tile_set(&self, tile_size: i32) -> Option<Gd<TileSet>> {
        let atlas_w = tile_size * ResourceKind::ALL.len() as i32;
        let atlas_h = tile_size;
        let Some(mut image) = Image::create(atlas_w, atlas_h, false, image::Format::RGBA8) else {
            godot_error!("GameWorld: failed to create resource node atlas image");
            return None;
        };
        image.fill(Color::from_rgba8(0, 0, 0, 0));

        for (index, kind) in ResourceKind::ALL.into_iter().enumerate() {
            self.draw_resource_node_tile(&mut image, tile_size, index as i32, kind);
        }

        let Some(texture) = ImageTexture::create_from_image(&image) else {
            godot_error!("GameWorld: failed to create resource node atlas texture");
            return None;
        };

        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let mut source = TileSetAtlasSource::new_gd();
        source.set_texture(&texture);
        source.set_texture_region_size(v2(tile_size, tile_size));
        for index in 0..ResourceKind::ALL.len() {
            source.create_tile_ex(v2(index as i32, 0)).done();
        }

        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(tile_size, tile_size));
        let source_ts = source.upcast::<TileSetSource>();
        let source_id = tile_set.add_source(&source_ts);
        if source_id < 0 {
            godot_error!("GameWorld: failed to add resource node atlas source");
            return None;
        }

        Some(tile_set)
    }

    fn draw_resource_node_tile(
        &self,
        image: &mut Gd<Image>,
        tile_size: i32,
        tile_index: i32,
        kind: ResourceKind,
    ) {
        let base_x = tile_index * tile_size;
        let color = match kind {
            ResourceKind::Wood => Color::from_rgb(0.18, 0.42, 0.16),
            ResourceKind::Stone => Color::from_rgb(0.55, 0.58, 0.6),
            ResourceKind::Food => Color::from_rgb(0.85, 0.23, 0.18),
            ResourceKind::Gold => Color::from_rgb(0.95, 0.72, 0.18),
        };
        let shadow = Color::from_rgba(0.02, 0.02, 0.02, 0.35);
        let highlight = Color::from_rgba(1.0, 1.0, 1.0, 0.35);
        let center = tile_size as f32 / 2.0;

        for y in 0..tile_size {
            for x in 0..tile_size {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                let distance = (dx * dx + dy * dy).sqrt();
                let radius = tile_size as f32 * 0.28;
                if distance <= radius {
                    image.set_pixel(base_x + x, y, color);
                } else if distance <= radius + 2.0 && dy > 0.0 {
                    image.set_pixel(base_x + x, y, shadow);
                }
            }
        }

        let glint_start_x = base_x + tile_size / 2 - tile_size / 10;
        let glint_start_y = tile_size / 2 - tile_size / 7;
        let glint_size = (tile_size / 10).max(2);
        for y in glint_start_y..glint_start_y + glint_size {
            for x in glint_start_x..glint_start_x + glint_size {
                image.set_pixel(x, y, highlight);
            }
        }
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
                .source_id(0)
                .atlas_coords(v2(kind as i32, 0))
                .done();
        }
        resource_map.update_internals();
    }

    fn resource_node_at(&mut self, coord: CellCoord) -> Option<ResourceKind> {
        self.game
            .with_surface_world_mut(self.rendered_surface, |world| {
                query_resource_nodes(world)
                    .into_iter()
                    .find_map(|(node_coord, kind)| (node_coord == coord).then_some(kind))
            })
            .flatten()
    }

    fn resource_nodes(&mut self) -> Option<Vec<(CellCoord, ResourceKind)>> {
        self.game
            .with_surface_world_mut(self.rendered_surface, query_resource_nodes)
    }

    fn switch_rendered_surface(&mut self, surface: SurfaceId) {
        if self.rendered_surface == surface {
            return;
        }

        self.rendered_surface = surface;
        self.selected_cell = None;

        let Some(mut tile_map) = self.tile_map_node() else {
            godot_error!("GameWorld: tile_map reference not set");
            self.disable_processing();
            return;
        };
        let Some(tile_source_id) = self.tile_source_id else {
            godot_error!("GameWorld: tile source id not initialized");
            self.disable_processing();
            return;
        };
        self.populate_tile_map(&mut tile_map, tile_source_id);

        let Some(mut resource_map) = self.resource_node_map_node() else {
            godot_error!("GameWorld: resource_node_map reference not set");
            self.disable_processing();
            return;
        };
        self.populate_resource_node_map(&mut resource_map);

        if let Some(mut camera) = self.camera_node() {
            self.configure_camera_for_surface(&mut camera);
        } else {
            godot_error!("GameWorld: camera reference not set");
            self.disable_processing();
            return;
        }

        let active_surface_index = surface_index_i32(self.rendered_surface);

        self.base_mut().queue_redraw();
        self.signals().tile_deselected().emit();
        self.signals().resources_changed().emit();
        self.signals().surface_changed().emit(active_surface_index);
    }
}

#[godot_api]
impl GameWorld {
    #[signal]
    pub(crate) fn tile_selected(x: i32, y: i32, type_name: GString, resource_name: GString);

    #[signal]
    pub(crate) fn tile_deselected();

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

    #[func]
    pub(crate) fn wood(&self) -> u32 {
        self.resource_amount(ResourceKind::Wood)
    }

    #[func]
    pub(crate) fn stone(&self) -> u32 {
        self.resource_amount(ResourceKind::Stone)
    }

    #[func]
    pub(crate) fn food(&self) -> u32 {
        self.resource_amount(ResourceKind::Food)
    }

    #[func]
    pub(crate) fn gold(&self) -> u32 {
        self.resource_amount(ResourceKind::Gold)
    }

    #[func]
    pub(crate) fn add_wood(&mut self, amount: u32) {
        self.add_resource(ResourceKind::Wood, amount);
    }

    #[func]
    pub(crate) fn add_stone(&mut self, amount: u32) {
        self.add_resource(ResourceKind::Stone, amount);
    }

    #[func]
    pub(crate) fn add_food(&mut self, amount: u32) {
        self.add_resource(ResourceKind::Food, amount);
    }

    #[func]
    pub(crate) fn add_gold(&mut self, amount: u32) {
        self.add_resource(ResourceKind::Gold, amount);
    }
}

fn surface_index_i32(surface: SurfaceId) -> i32 {
    i32::try_from(surface.index()).unwrap_or(i32::MAX)
}

fn query_resource_nodes(world: &mut World) -> Vec<(CellCoord, ResourceKind)> {
    let mut query = world.query::<(&TilePosition, &ResourceNode)>();
    query
        .iter(world)
        .map(|(position, node)| (position.coord, node.kind))
        .collect()
}
