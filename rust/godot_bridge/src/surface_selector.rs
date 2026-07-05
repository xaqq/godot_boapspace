use crate::game_world::GameWorld;
use godot::classes::{control, Button, IPanelContainer, PanelContainer, VBoxContainer};
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct SurfaceSelector {
    #[export]
    button_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    surface_buttons: Vec<Gd<Button>>,
    cached_surface_count: i32,
    cached_active_surface_index: i32,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for SurfaceSelector {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            button_container: OnEditor::default(),
            game_world: OnEditor::default(),
            surface_buttons: Vec::new(),
            cached_surface_count: -1,
            cached_active_surface_index: -1,
            base,
        }
    }

    fn ready(&mut self) {
        self.rebuild_buttons();
        self.refresh_button_states();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let game_world = self.game_world.clone();

        let surface_count = game_world.bind().surface_count();
        if surface_count != self.cached_surface_count {
            self.rebuild_buttons();
        }

        let active_surface_index = game_world.bind().active_surface_index();
        if active_surface_index != self.cached_active_surface_index {
            self.refresh_button_states();
        }
    }
}

impl SurfaceSelector {
    fn rebuild_buttons(&mut self) {
        let game_world = self.game_world.clone();
        let mut button_container = self.button_container.clone();

        for mut button in self.surface_buttons.drain(..) {
            button.queue_free();
        }

        let surface_count = game_world.bind().surface_count();
        for index in 0..surface_count {
            let mut button = Button::new_alloc();
            button.set_name(format!("SurfaceButton{}", index + 1).as_str());
            button.set_text(format!("Surface {}", index + 1).as_str());
            button.set_h_size_flags(control::SizeFlags::EXPAND_FILL);

            button.signals().pressed().connect_other(
                &game_world,
                move |game_world: &mut GameWorld| {
                    game_world.set_active_surface_index(index);
                },
            );

            button_container.add_child(&button);
            self.surface_buttons.push(button);
        }

        self.cached_surface_count = surface_count;
        self.cached_active_surface_index = -1;
        self.refresh_button_states();
    }

    fn refresh_button_states(&mut self) {
        let game_world = self.game_world.clone();
        let active_surface_index = game_world.bind().active_surface_index();

        for (index, button) in self.surface_buttons.iter_mut().enumerate() {
            button.set_disabled(i32::try_from(index) == Ok(active_surface_index));
        }

        self.cached_active_surface_index = active_surface_index;
    }
}
