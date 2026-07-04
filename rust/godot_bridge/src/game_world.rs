use crate::game_state::GameState;
use game_engine::grid::{self, Grid};
use game_engine::resources::{GameResources, ResourceKind};
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

#[derive(GodotClass)]
#[class(base = Node2D)]
pub(crate) struct GameWorld {
    #[export]
    tile_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    camera: OnEditor<Gd<Camera2D>>,

    game_state: GameState,
    selected_cell: Option<(i32, i32, GString)>,
    tile_source_id: i32,
    _tile_set: Option<Gd<TileSet>>,

    base: Base<Node2D>,
}

#[godot_api]
impl INode2D for GameWorld {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            tile_map: OnEditor::default(),
            camera: OnEditor::default(),
            game_state: GameState::new(),
            selected_cell: None,
            tile_source_id: -1,
            _tile_set: None,
            base,
        }
    }

    fn ready(&mut self) {
        let mut tm = self.tile_map.clone();
        let mut cam = self.camera.clone();

        let ts = grid::TILE_SIZE as i32;
        let atlas_w = ts * 2;
        let atlas_h = ts;

        let mut image = Image::create(atlas_w, atlas_h, false, image::Format::RGBA8).unwrap();
        image.fill(Color::from_rgba8(0, 0, 0, 0));

        let grass = Color::from_rgb(0.25, 0.55, 0.15);
        let brown = Color::from_rgb(0.55, 0.45, 0.25);

        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let rect = |x: i32, y: i32, w: i32, h: i32| Rect2i::new(v2(x, y), v2(w, h));

        image.fill_rect(rect(0, 0, ts, ts), grass);

        let bx = ts;
        image.fill_rect(rect(bx, 0, ts, ts), brown);

        let texture = ImageTexture::create_from_image(&image).unwrap();

        let mut source = TileSetAtlasSource::new_gd();
        source.set_texture(&texture);
        source.set_texture_region_size(v2(ts, ts));
        source.create_tile_ex(v2(0, 0)).done();
        source.create_tile_ex(v2(1, 0)).done();

        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(ts, ts));
        let source_ts = source.upcast::<TileSetSource>();
        self.tile_source_id = tile_set.add_source(&source_ts);

        tm.set_tile_set(&tile_set);
        self._tile_set = Some(tile_set);
        tm.set_navigation_enabled(false);
        tm.set_texture_filter(TextureFilter::NEAREST);
        tm.set_draw_behind_parent(true);

        let grid = self.game_state.world.resource::<Grid>();
        let w = grid.width as i32;
        let h = grid.height as i32;
        for y in 0..h {
            for x in 0..w {
                tm.set_cell_ex(v2(x, y))
                    .source_id(self.tile_source_id)
                    .atlas_coords(v2(0, 0))
                    .done();
            }
        }
        tm.update_internals();

        cam.set_enabled(true);
        cam.make_current();
        cam.set_position(self.world_size() / 2.0);
        cam.set_zoom(Vector2::new(0.5, 0.5));
        cam.set_limit(Side::LEFT, 0);
        cam.set_limit(Side::TOP, 0);
        cam.set_limit(Side::RIGHT, self.world_size().x as i32);
        cam.set_limit(Side::BOTTOM, self.world_size().y as i32);
        cam.set_limit_smoothing_enabled(false);
        cam.set_position_smoothing_enabled(false);

        self.base_mut().set_process_input(true);
        self.base_mut().set_process(true);
        self.base_mut().queue_redraw();
    }

    fn process(&mut self, delta: f64) {
        let input = Input::singleton();

        let vs = self.get_viewport_size();
        let ws = self.world_size();
        let min_zoom = {
            let fit_x = vs.x / ws.x;
            let fit_y = vs.y / ws.y;
            (fit_x.max(fit_y) * ZOOM_MARGIN).max(ZOOM_ABSOLUTE_FLOOR)
        };

        {
            let mut cam = self.camera();
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
            let zoom = self.camera().get_zoom().x;
            let speed = PAN_SPEED / zoom;
            let mut cam = self.camera();
            let pos = cam.get_position();
            cam.set_position(pos + dir * speed * delta as f32);
        }

        self.game_state.tick(delta as f32);
    }

    fn draw(&mut self) {
        let ts = grid::TILE_SIZE;
        let ws = self.world_size();
        let grid_resource = self.game_state.world.resource::<Grid>();
        let w = grid_resource.width as i32;
        let h = grid_resource.height as i32;
        let grid_color = Color::from_rgb(0.12, 0.35, 0.05);
        let hl_cell = self.selected_cell.as_ref().map(|(x, y, _)| (*x, *y));

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

        if let Some((cx, cy)) = hl_cell {
            let cell_pos = Vector2::new(cx as f32 * ts, cy as f32 * ts);
            let cell_size = Vector2::new(ts, ts);
            let yellow = Color::from_rgb(1.0, 0.84, 0.0);
            let mut fill = yellow;
            fill.a = 0.15;
            base.draw_rect_ex(Rect2::new(cell_pos, cell_size), fill)
                .filled(true)
                .done();
            base.draw_rect_ex(Rect2::new(cell_pos, cell_size), yellow)
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
    fn camera(&self) -> Gd<Camera2D> {
        self.camera.clone()
    }

    fn get_viewport_size(&self) -> Vector2 {
        self.base()
            .get_viewport()
            .map(|vp| vp.get_visible_rect().size)
            .unwrap_or_else(|| Vector2::new(1920.0, 1080.0))
    }

    fn world_size(&self) -> Vector2 {
        let grid = self.game_state.world.resource::<Grid>();
        Vector2::new(
            grid.width as f32 * grid::TILE_SIZE,
            grid.height as f32 * grid::TILE_SIZE,
        )
    }

    fn handle_tile_click(&mut self) {
        let mouse_pos = self.base().get_local_mouse_position();

        let grid = self.game_state.world.resource::<Grid>();
        if let Some((cx, cy)) = Grid::world_to_cell(
            mouse_pos.x,
            mouse_pos.y,
            grid.width as i32,
            grid.height as i32,
        ) {
            let current = &self.selected_cell;
            if current.as_ref().map(|(x, y, _)| (*x, *y)) == Some((cx, cy)) {
                self.selected_cell = None;
                self.base_mut().queue_redraw();
                self.signals().tile_deselected().emit();
            } else {
                let type_name = grid
                    .get(cx, cy)
                    .map(|c| GString::from(c.type_name()))
                    .unwrap_or_default();
                self.selected_cell = Some((cx, cy, type_name.clone()));
                self.base_mut().queue_redraw();
                self.signals().tile_selected().emit(cx, cy, &type_name);
            }
        } else {
            self.selected_cell = None;
            self.base_mut().queue_redraw();
            self.signals().tile_deselected().emit();
        }
    }

    fn handle_mouse_wheel(&mut self, factor: f32) {
        let old_zoom = self.camera().get_zoom().x;

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

        let world_under_cursor = self.camera().get_position() + cursor_offset / old_zoom;

        let mut cam = self.camera();
        cam.set_zoom(Vector2::new(new_zoom, new_zoom));
        cam.set_position(world_under_cursor - cursor_offset / new_zoom);
    }
}

#[godot_api]
impl GameWorld {
    #[signal]
    pub(crate) fn tile_selected(x: i32, y: i32, type_name: GString);

    #[signal]
    pub(crate) fn tile_deselected();

    #[signal]
    pub(crate) fn resources_changed();

    #[func]
    pub(crate) fn wood(&self) -> u32 {
        self.game_state.world.resource::<GameResources>().wood
    }

    #[func]
    pub(crate) fn stone(&self) -> u32 {
        self.game_state.world.resource::<GameResources>().stone
    }

    #[func]
    pub(crate) fn food(&self) -> u32 {
        self.game_state.world.resource::<GameResources>().food
    }

    #[func]
    pub(crate) fn gold(&self) -> u32 {
        self.game_state.world.resource::<GameResources>().gold
    }

    #[func]
    pub(crate) fn add_wood(&mut self, amount: u32) {
        self.game_state
            .world
            .resource_mut::<GameResources>()
            .add(ResourceKind::Wood, amount);
        self.signals().resources_changed().emit();
    }

    #[func]
    pub(crate) fn add_stone(&mut self, amount: u32) {
        self.game_state
            .world
            .resource_mut::<GameResources>()
            .add(ResourceKind::Stone, amount);
        self.signals().resources_changed().emit();
    }

    #[func]
    pub(crate) fn add_food(&mut self, amount: u32) {
        self.game_state
            .world
            .resource_mut::<GameResources>()
            .add(ResourceKind::Food, amount);
        self.signals().resources_changed().emit();
    }

    #[func]
    pub(crate) fn add_gold(&mut self, amount: u32) {
        self.game_state
            .world
            .resource_mut::<GameResources>()
            .add(ResourceKind::Gold, amount);
        self.signals().resources_changed().emit();
    }
}
