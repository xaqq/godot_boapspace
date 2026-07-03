use godot::classes::{
    Control, IControl, Input, InputEvent, InputEventKey,
    InputEventMouseButton,
};
use godot::global::{Key, MouseButton};
use godot::obj::OnEditor;
use godot::prelude::*;
use crate::game_surface::{self, GameSurface};
use crate::ingame_menu::IngameMenu;

const ZOOM_MIN: f32 = 0.0625;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_FACTOR: f32 = 1.1;
const PAN_SPEED: f32 = 600.0;

#[derive(GodotClass)]
#[class(base = Control)]
struct GameWorld {
    #[export]
    ingame_menu: OnEditor<Gd<IngameMenu>>,

    surface: GameSurface,
    camera_center: Vector2,
    camera_zoom: f32,
    viewport_size: Vector2,

    base: Base<Control>,
}

#[godot_api]
impl IControl for GameWorld {
    fn init(base: Base<Control>) -> Self {
        let surface = GameSurface::new(1024, 1024);
        Self {
            ingame_menu: OnEditor::default(),
            camera_center: surface.world_size() / 2.0,
            camera_zoom: 0.5,
            viewport_size: Vector2::ZERO,
            surface,
            base,
        }
    }

    fn ready(&mut self) {
        self.ingame_menu.clone().set_visible(false);
        self.read_viewport_size();
        self.base_mut().set_process(true);
        self.base_mut().queue_redraw();
    }

    fn process(&mut self, _delta: f64) {
        self.read_viewport_size();

        let input = Input::singleton();

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
            let speed = PAN_SPEED / self.camera_zoom;
            self.camera_center += dir * speed * _delta as f32;
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let mut vs = self.viewport_size;
        if vs.x <= 0.0 || vs.y <= 0.0 {
            vs = self.base().get_size();
        }
        if vs.x <= 0.0 || vs.y <= 0.0 {
            return;
        }

        let tile_size = game_surface::TILE_SIZE;
        let zoom = self.camera_zoom;
        let center = self.camera_center;

        let ts = tile_size * zoom;
        let visible_offset = center - vs / 2.0 / zoom;

        let start_x = (visible_offset.x / tile_size).floor().max(0.0) as i32;
        let start_y = (visible_offset.y / tile_size).floor().max(0.0) as i32;
        let end_x = ((visible_offset.x + vs.x / zoom) / tile_size).ceil() as i32 + 1;
        let end_y = ((visible_offset.y + vs.y / zoom) / tile_size).ceil() as i32 + 1;

        let grass = Color::from_rgb(0.25, 0.55, 0.15);
        let line = Color::from_rgb(0.12, 0.35, 0.05);
        let line_w = 1.0 / zoom;

        for cy in start_y..end_y {
            for cx in start_x..end_x {
                let x = (cx as f32 * tile_size - visible_offset.x) * zoom;
                let y = (cy as f32 * tile_size - visible_offset.y) * zoom;

                self.base_mut()
                    .draw_rect_ex(
                        Rect2::new(Vector2::new(x, y), Vector2::new(ts, ts)),
                        grass,
                    )
                    .done();
            }
        }

        for cy in start_y..=end_y {
            let y = (cy as f32 * tile_size - visible_offset.y) * zoom;
            let x0 = (start_x as f32 * tile_size - visible_offset.x) * zoom;
            let width = (end_x - start_x) as f32 * ts;
            self.base_mut()
                .draw_rect_ex(
                    Rect2::new(Vector2::new(x0, y - line_w), Vector2::new(width, line_w * 2.0)),
                    line,
                )
                .done();
        }

        for cx in start_x..=end_x {
            let x = (cx as f32 * tile_size - visible_offset.x) * zoom;
            let y0 = (start_y as f32 * tile_size - visible_offset.y) * zoom;
            let height = (end_y - start_y) as f32 * ts;
            self.base_mut()
              .draw_rect_ex(
                    Rect2::new(Vector2::new(x - line_w, y0), Vector2::new(line_w * 2.0, height)),
                    line,
                )
                .done();
        }

        let world_w = self.surface.world_size().x;
        let world_h = self.surface.world_size().y;
        let screen_x = -visible_offset.x * zoom;
        let screen_y = -visible_offset.y * zoom;
        let screen_w = world_w * zoom;
        let screen_h = world_h * zoom;
        let border_w = 3.0 / zoom;
        let border_color = Color::from_rgb(0.35, 0.18, 0.05);

        self.base_mut()
            .draw_rect_ex(
                Rect2::new(
                    Vector2::new(screen_x, screen_y),
                    Vector2::new(screen_w, screen_h),
                ),
                border_color,
            )
            .filled(false)
            .width(border_w)
            .done();
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        let Ok(key_event) = event.clone().try_cast::<InputEventKey>() else {
            self.handle_mouse_wheel(event);
            return;
        };

        if key_event.get_keycode() == Key::ESCAPE
            && key_event.is_pressed()
            && !key_event.is_echo()
        {
            let mut menu = self.ingame_menu.clone();
            let visible = menu.is_visible();
            menu.set_visible(!visible);
        }
    }
}

impl GameWorld {
    fn read_viewport_size(&mut self) {
        if let Some(vp) = self.base().get_viewport() {
            self.viewport_size = vp.get_visible_rect().size;
        }
        if self.viewport_size.x <= 0.0 || self.viewport_size.y <= 0.0 {
            self.viewport_size = self.base().get_size();
        }
    }

    fn handle_mouse_wheel(&mut self, event: Gd<InputEvent>) {
        let Ok(wheel) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !wheel.is_pressed() {
            return;
        }

        let factor = match wheel.get_button_index() {
            MouseButton::WHEEL_UP => ZOOM_FACTOR,
            MouseButton::WHEEL_DOWN => 1.0 / ZOOM_FACTOR,
            _ => return,
        };

        let new_zoom = self.camera_zoom * factor;
        if new_zoom < ZOOM_MIN || new_zoom > ZOOM_MAX {
            return;
        }

        let mouse_pos = self.base().get_local_mouse_position();
        let half_vs = self.viewport_size / 2.0;
        let cursor_offset = mouse_pos - half_vs;

        let world_under_cursor =
            self.camera_center + cursor_offset / self.camera_zoom;
        self.camera_zoom = new_zoom;
        self.camera_center = world_under_cursor - cursor_offset / new_zoom;

        self.base_mut().queue_redraw();
    }
}
