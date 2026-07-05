use crate::world::game_world::GameWorld;
use godot::classes::{Button, HBoxContainer, IHBoxContainer, Label};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
pub(crate) struct SimulationHeaderBar {
    #[export]
    play_pause_button: OnEditor<Gd<Button>>,

    #[export]
    speed_1x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_2x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_4x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_50x_button: OnEditor<Gd<Button>>,

    #[export]
    speed_100x_button: OnEditor<Gd<Button>>,

    #[export]
    datetime_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for SimulationHeaderBar {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            play_pause_button: OnEditor::default(),
            speed_1x_button: OnEditor::default(),
            speed_2x_button: OnEditor::default(),
            speed_4x_button: OnEditor::default(),
            speed_50x_button: OnEditor::default(),
            speed_100x_button: OnEditor::default(),
            datetime_label: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let play_pause_button = self.play_pause_button.clone();
        let speed_1x_button = self.speed_1x_button.clone();
        let speed_2x_button = self.speed_2x_button.clone();
        let speed_4x_button = self.speed_4x_button.clone();
        let speed_50x_button = self.speed_50x_button.clone();
        let speed_100x_button = self.speed_100x_button.clone();
        let game_world = self.game_world.clone();

        play_pause_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.toggle_simulation_playing();
            },
        );

        speed_1x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(1);
            },
        );

        speed_2x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(2);
            },
        );

        speed_4x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(4);
            },
        );

        speed_50x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(50);
            },
        );

        speed_100x_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.set_simulation_speed_multiplier(100);
            },
        );

        self.refresh_controls();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        self.refresh_controls();
    }
}

impl SimulationHeaderBar {
    fn refresh_controls(&mut self) {
        let game_world = self.game_world.clone();
        let game_world = game_world.bind();
        let is_playing = game_world.is_simulation_playing();
        let datetime_text = game_world.simulation_datetime_text_string();
        let simulation_speed_multiplier = game_world.simulation_speed_multiplier();
        drop(game_world);

        let mut play_pause_button = self.play_pause_button.clone();
        play_pause_button.set_text(play_pause_text(is_playing));

        self.refresh_speed_button_states(simulation_speed_multiplier);

        let mut datetime_label = self.datetime_label.clone();
        datetime_label.set_text(datetime_text.as_str());
    }

    fn refresh_speed_button_states(&mut self, active_multiplier: i32) {
        let mut speed_1x_button = self.speed_1x_button.clone();
        speed_1x_button.set_disabled(speed_button_disabled(1, active_multiplier));

        let mut speed_2x_button = self.speed_2x_button.clone();
        speed_2x_button.set_disabled(speed_button_disabled(2, active_multiplier));

        let mut speed_4x_button = self.speed_4x_button.clone();
        speed_4x_button.set_disabled(speed_button_disabled(4, active_multiplier));

        let mut speed_50x_button = self.speed_50x_button.clone();
        speed_50x_button.set_disabled(speed_button_disabled(50, active_multiplier));

        let mut speed_100x_button = self.speed_100x_button.clone();
        speed_100x_button.set_disabled(speed_button_disabled(100, active_multiplier));
    }
}

fn play_pause_text(is_playing: bool) -> &'static str {
    if is_playing {
        "Pause"
    } else {
        "Play"
    }
}

fn speed_button_disabled(button_multiplier: i32, active_multiplier: i32) -> bool {
    button_multiplier == active_multiplier
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_pause_text_matches_next_action() {
        assert_eq!(play_pause_text(true), "Pause");
        assert_eq!(play_pause_text(false), "Play");
    }

    #[test]
    fn active_speed_button_is_disabled() {
        assert!(speed_button_disabled(2, 2));
        assert!(!speed_button_disabled(1, 2));
        assert!(!speed_button_disabled(4, 2));
        assert!(!speed_button_disabled(50, 2));
        assert!(!speed_button_disabled(100, 2));
    }
}
