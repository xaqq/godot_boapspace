use godot::builtin::Side;
use godot::classes::{
    image, Camera2D, INode2D, Image, ImageTexture, Input, InputEvent,
    InputEventMouseButton, Node2D, TileMapLayer, TileSet, TileSetAtlasSource,
    TileSetSource,
};
use godot::global::{Key, MouseButton};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::game_surface::{self, GameSurface};
use crate::selected_tile::SelectedTile;

const ZOOM_ABSOLUTE_FLOOR: f32 = 0.001;
const ZOOM_MARGIN: f32 = 0.95;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_FACTOR: f32 = 1.1;
const PAN_SPEED: f32 = 600.0;

#[derive(GodotClass)]
#[class(base = Node2D)]
pub(crate) struct GameWorld {
    #[export]
    selected_tile: OnEditor<Gd<SelectedTile>>,

    #[export]
    tile_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    camera: OnEditor<Gd<Camera2D>>,

    surface: GameSurface,
    prev_highlight: Option<(i32, i32)>,
    tile_source_id: i32,
    _tile_set: Option<Gd<TileSet>>,

    #[export]
    highlight_layer: OnEditor<Gd<TileMapLayer>>,

    base: Base<Node2D>,
}

#[godot_api]
impl INode2D for GameWorld {
    fn init(base: Base<Node2D>) -> Self {
        let surface = GameSurface::new(256, 256);
        Self {
            selected_tile: OnEditor::default(),
            tile_map: OnEditor::default(),
            camera: OnEditor::default(),
            surface,
            prev_highlight: None,
            tile_source_id: -1,
            _tile_set: None,
            highlight_layer: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let ts = game_surface::TILE_SIZE as i32;
        let atlas_w = ts * 3;
        let atlas_h = ts;

        let tm = self.base().get_node_as::<TileMapLayer>("TileMap");
        let hl = self.base().get_node_as::<TileMapLayer>("HighlightLayer");
        let cam = self.base().get_node_as::<Camera2D>("Camera2D");
        *self.tile_map = tm;
        *self.highlight_layer = hl;
        *self.camera = cam;

        let mut tm = self.tile_map.clone();
        let mut cam = self.camera.clone();

        let mut image =
            Image::create(atlas_w, atlas_h, false, image::Format::RGBA8).unwrap();
        image.fill(Color::from_rgba8(0, 0, 0, 0));

        let grass = Color::from_rgb(0.25, 0.55, 0.15);
        let dark = Color::from_rgb(0.12, 0.35, 0.05);
        let brown = Color::from_rgb(0.55, 0.45, 0.25);
        let yellow = Color::from_rgb(1.0, 0.84, 0.0);

        let v2 = |x: i32, y: i32| Vector2i::new(x, y);
        let rect = |x: i32, y: i32, w: i32, h: i32| Rect2i::new(v2(x, y), v2(w, h));

        image.fill_rect(rect(0, 0, ts, ts), grass);
        image.fill_rect(rect(ts - 1, 0, 1, ts), dark);
        image.fill_rect(rect(0, ts - 1, ts, 1), dark);

        let bx = ts;
        image.fill_rect(rect(bx, 0, ts, ts), brown);
        image.fill_rect(rect(bx + ts - 1, 0, 1, ts), dark);
        image.fill_rect(rect(bx, ts - 1, ts, 1), dark);

        let hx = ts * 2;
        let bw: i32 = 3;
        image.fill_rect(rect(hx, 0, ts, bw), yellow);
        image.fill_rect(rect(hx, ts - bw, ts, bw), yellow);
        image.fill_rect(rect(hx, 0, bw, ts), yellow);
        image.fill_rect(rect(hx + ts - bw, 0, bw, ts), yellow);

        let texture = ImageTexture::create_from_image(&image).unwrap();

        let mut source = TileSetAtlasSource::new_gd();
        source.set_texture(&texture);
        source.set_texture_region_size(v2(ts, ts));
        source.create_tile_ex(v2(0, 0)).done();
        source.create_tile_ex(v2(1, 0)).done();
        source.create_tile_ex(v2(2, 0)).done();

        let mut tile_set = TileSet::new_gd();
        tile_set.set_tile_size(v2(ts, ts));
        let source_ts = source.upcast::<TileSetSource>();
        self.tile_source_id = tile_set.add_source(&source_ts);

        tm.set_tile_set(&tile_set);
        self._tile_set = Some(tile_set);
        tm.set_navigation_enabled(false);

        let w = self.surface.width as i32;
        let h = self.surface.height as i32;
        for y in 0..h {
            for x in 0..w {
                tm.set_cell_ex(v2(x, y))
                    .source_id(self.tile_source_id)
                    .atlas_coords(v2(0, 0))
                    .done();
            }
        }
        tm.update_internals();

        let mut hl = self.highlight_layer.clone();
        hl.set_tile_set(self._tile_set.as_ref().unwrap());
        hl.set_navigation_enabled(false);

        cam.set_enabled(true);
        cam.make_current();
        cam.set_position(self.surface.world_size() / 2.0);
        cam.set_zoom(Vector2::new(0.5, 0.5));
        cam.set_limit(Side::LEFT, 0);
        cam.set_limit(Side::TOP, 0);
        cam.set_limit(Side::RIGHT, self.surface.world_size().x as i32);
        cam.set_limit(Side::BOTTOM, self.surface.world_size().y as i32);
        cam.set_limit_smoothing_enabled(false);
        cam.set_position_smoothing_enabled(false);

        self.base_mut().set_process_input(true);
        self.base_mut().set_process(true);
        self.base_mut().queue_redraw();
    }

    fn process(&mut self, delta: f64) {
        let input = Input::singleton();

        let vs = self.get_viewport_size();
        let min_zoom = {
            let fit_x = vs.x / self.surface.world_size().x;
            let fit_y = vs.y / self.surface.world_size().y;
            (fit_x.min(fit_y) * ZOOM_MARGIN).max(ZOOM_ABSOLUTE_FLOOR)
        };

        {
            let mut cam = self.camera.clone();
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
            let zoom = self.camera.clone().get_zoom().x;
            let speed = PAN_SPEED / zoom;
            let mut cam = self.camera.clone();
            let pos = cam.get_position();
            cam.set_position(pos + dir * speed * delta as f32);
        }
    }

    fn draw(&mut self) {
        let ws = self.surface.world_size();
        self.base_mut()
            .draw_rect_ex(
                Rect2::new(Vector2::ZERO, ws),
                Color::from_rgb(0.95, 0.35, 0.05),
            )
            .filled(false)
            .width(4.0)
            .done();
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
    fn get_viewport_size(&self) -> Vector2 {
        self.base()
            .get_viewport()
            .map(|vp| vp.get_visible_rect().size)
            .unwrap_or_else(|| Vector2::new(1920.0, 1080.0))
    }

    fn handle_tile_click(&mut self) {
        let mouse_pos = self.base().get_local_mouse_position();

        let mut tile = self.selected_tile.clone();
        let mut bound = tile.bind_mut();

        if let Some((cx, cy)) = GameSurface::world_to_cell(
            mouse_pos,
            self.surface.width as i32,
            self.surface.height as i32,
        ) {
            let current = (bound.cell_x(), bound.cell_y());
            if current == (Some(cx), Some(cy)) {
                bound.deselect();
                self.set_highlight(None);
            } else {
                let type_name = self
                    .surface
                    .get(cx, cy)
                    .map(|c| GString::from(c.type_name()))
                    .unwrap_or_default();
                bound.select(cx, cy, type_name);
                self.set_highlight(Some((cx, cy)));
            }
        } else {
            bound.deselect();
            self.set_highlight(None);
        }
    }

    fn handle_mouse_wheel(&mut self, factor: f32) {
        let old_zoom = self.camera.clone().get_zoom().x;

        let vs = self.get_viewport_size();
        let min_zoom = {
            let fit_x = vs.x / self.surface.world_size().x;
            let fit_y = vs.y / self.surface.world_size().y;
            (fit_x.min(fit_y) * ZOOM_MARGIN).max(ZOOM_ABSOLUTE_FLOOR)
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

        let world_under_cursor =
            self.camera.clone().get_position() + cursor_offset / old_zoom;

        let mut cam = self.camera.clone();
        cam.set_zoom(Vector2::new(new_zoom, new_zoom));
        cam.set_position(world_under_cursor - cursor_offset / new_zoom);
    }

    fn set_highlight(&mut self, cell: Option<(i32, i32)>) {
        let mut hl = self.highlight_layer.clone();

        if let Some((x, y)) = self.prev_highlight {
            hl.erase_cell(Vector2i::new(x, y));
        }

        if let Some((x, y)) = cell {
            hl.set_cell_ex(Vector2i::new(x, y))
                .source_id(self.tile_source_id)
                .atlas_coords(Vector2i::new(2, 0))
                .done();
        }

        self.prev_highlight = cell;
    }
}

#[godot_api]
impl GameWorld {}
