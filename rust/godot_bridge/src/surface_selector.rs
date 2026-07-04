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
        if self.game_world_node().is_none() {
            godot_warn!("SurfaceSelector: game_world reference not set");
            return;
        }
        if self.button_container_node().is_none() {
            godot_warn!("SurfaceSelector: button_container reference not set");
            return;
        }

        self.rebuild_buttons();
        self.refresh_button_states();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let Some(game_world) = self.game_world_node() else {
            return;
        };

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
    fn game_world_node(&self) -> Option<Gd<GameWorld>> {
        let game_world = self.game_world.clone();
        game_world.is_instance_valid().then_some(game_world)
    }

    fn button_container_node(&self) -> Option<Gd<VBoxContainer>> {
        let button_container = self.button_container.clone();
        button_container
            .is_instance_valid()
            .then_some(button_container)
    }

    fn rebuild_buttons(&mut self) {
        let Some(game_world) = self.game_world_node() else {
            return;
        };
        let Some(mut button_container) = self.button_container_node() else {
            return;
        };

        for mut button in self.surface_buttons.drain(..) {
            if button.is_instance_valid() {
                button.queue_free();
            }
        }

        let surface_count = game_world.bind().surface_count();
        for index in 0..surface_count {
            let mut button = Button::new_alloc();
            button.set_name(format!("SurfaceButton{}", index + 1).as_str());
            button.set_text(format!("Surface {}", index + 1).as_str());
            button.set_h_size_flags(control::SizeFlags::EXPAND_FILL);

            let mut selected_game_world = game_world.clone();
            button.signals().pressed().connect(move || {
                if selected_game_world.is_instance_valid() {
                    selected_game_world
                        .bind_mut()
                        .set_active_surface_index(index);
                }
            });

            button_container.add_child(&button);
            self.surface_buttons.push(button);
        }

        self.cached_surface_count = surface_count;
        self.cached_active_surface_index = -1;
        self.refresh_button_states();
    }

    fn refresh_button_states(&mut self) {
        let Some(game_world) = self.game_world_node() else {
            return;
        };
        let active_surface_index = game_world.bind().active_surface_index();

        for (index, button) in self.surface_buttons.iter_mut().enumerate() {
            if button.is_instance_valid() {
                button.set_disabled(i32::try_from(index) == Ok(active_surface_index));
            }
        }

        self.cached_active_surface_index = active_surface_index;
    }
}
