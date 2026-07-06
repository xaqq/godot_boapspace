use crate::world::game_world::GameWorld;
use godot::classes::{Button, IPanelContainer, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct BuildingPalette {
    #[export]
    warehouse_button: OnEditor<Gd<Button>>,

    #[export]
    town_hall_button: OnEditor<Gd<Button>>,

    #[export]
    farm_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for BuildingPalette {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            warehouse_button: OnEditor::default(),
            town_hall_button: OnEditor::default(),
            farm_button: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let warehouse_button = self.warehouse_button.clone();
        let town_hall_button = self.town_hall_button.clone();
        let farm_button = self.farm_button.clone();
        let game_world = self.game_world.clone();

        warehouse_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_warehouse_blueprint_placement();
            },
        );

        town_hall_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_town_hall_blueprint_placement();
            },
        );

        farm_button
            .signals()
            .pressed()
            .connect_other(&game_world, |game_world: &mut GameWorld| {
                game_world.start_farm_blueprint_placement();
            });
    }
}
