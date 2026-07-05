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
            datetime_label: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let play_pause_button = self.play_pause_button.clone();
        let game_world = self.game_world.clone();

        play_pause_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.toggle_simulation_playing();
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
        drop(game_world);

        let mut play_pause_button = self.play_pause_button.clone();
        play_pause_button.set_text(play_pause_text(is_playing));

        let mut datetime_label = self.datetime_label.clone();
        datetime_label.set_text(datetime_text.as_str());
    }
}

fn play_pause_text(is_playing: bool) -> &'static str {
    if is_playing {
        "Pause"
    } else {
        "Play"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_pause_text_matches_next_action() {
        assert_eq!(play_pause_text(true), "Pause");
        assert_eq!(play_pause_text(false), "Play");
    }
}
