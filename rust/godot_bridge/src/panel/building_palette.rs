use crate::world::game_world::GameWorld;
use godot::classes::{Button, IPanelContainer, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct BuildingPalette {
    #[export]
    depot_button: OnEditor<Gd<Button>>,

    #[export]
    warehouse_button: OnEditor<Gd<Button>>,

    #[export]
    town_hall_button: OnEditor<Gd<Button>>,

    #[export]
    sawmill_button: OnEditor<Gd<Button>>,

    #[export]
    stoneworks_button: OnEditor<Gd<Button>>,

    #[export]
    kitchen_button: OnEditor<Gd<Button>>,

    #[export]
    farm_button: OnEditor<Gd<Button>>,

    #[export]
    forester_lodge_button: OnEditor<Gd<Button>>,

    #[export]
    small_house_button: OnEditor<Gd<Button>>,

    #[export]
    medium_house_button: OnEditor<Gd<Button>>,

    #[export]
    large_house_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for BuildingPalette {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            depot_button: OnEditor::default(),
            warehouse_button: OnEditor::default(),
            town_hall_button: OnEditor::default(),
            sawmill_button: OnEditor::default(),
            stoneworks_button: OnEditor::default(),
            kitchen_button: OnEditor::default(),
            farm_button: OnEditor::default(),
            forester_lodge_button: OnEditor::default(),
            small_house_button: OnEditor::default(),
            medium_house_button: OnEditor::default(),
            large_house_button: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let depot_button = self.depot_button.clone();
        let warehouse_button = self.warehouse_button.clone();
        let town_hall_button = self.town_hall_button.clone();
        let sawmill_button = self.sawmill_button.clone();
        let stoneworks_button = self.stoneworks_button.clone();
        let kitchen_button = self.kitchen_button.clone();
        let farm_button = self.farm_button.clone();
        let forester_lodge_button = self.forester_lodge_button.clone();
        let small_house_button = self.small_house_button.clone();
        let medium_house_button = self.medium_house_button.clone();
        let large_house_button = self.large_house_button.clone();
        let game_world = self.game_world.clone();

        depot_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_depot_blueprint_placement();
            },
        );

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

        sawmill_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_sawmill_blueprint_placement();
            },
        );

        stoneworks_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_stoneworks_blueprint_placement();
            },
        );

        kitchen_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_kitchen_blueprint_placement();
            },
        );

        farm_button
            .signals()
            .pressed()
            .connect_other(&game_world, |game_world: &mut GameWorld| {
                game_world.start_farm_blueprint_placement();
            });

        forester_lodge_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_forester_lodge_blueprint_placement();
            },
        );

        small_house_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_small_house_blueprint_placement();
            },
        );

        medium_house_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_medium_house_blueprint_placement();
            },
        );

        large_house_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_large_house_blueprint_placement();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    const PALETTE_SCENE: &str = include_str!("../../../../godot/panel/building_palette.tscn");

    #[test]
    fn palette_scene_uses_the_specified_major_building_order() {
        let button_names = [
            "DepotButton",
            "WarehouseButton",
            "TownHallButton",
            "SawmillButton",
            "StoneworksButton",
            "KitchenButton",
            "FarmButton",
            "ForesterLodgeButton",
            "SmallHouseButton",
            "MediumHouseButton",
            "LargeHouseButton",
        ];

        let positions = button_names.map(|button_name| {
            PALETTE_SCENE
                .find(format!("[node name=\"{button_name}\"").as_str())
                .unwrap_or_else(|| panic!("palette scene is missing {button_name}"))
        });

        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
