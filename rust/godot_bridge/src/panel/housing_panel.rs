use crate::world::game_world::GameWorld;
use game_engine::buildings::BuildingKind;
use game_engine::housing::housing_snapshot;
use godot::classes::{Button, IPanelContainer, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct HousingTierRow {
    homes: usize,
    occupied: usize,
    capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct HousingOverview {
    small: HousingTierRow,
    medium: HousingTierRow,
    large: HousingTierRow,
    total_homes: usize,
    total_occupied: usize,
    total_capacity: usize,
    homeless: usize,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct HousingPanel {
    #[export]
    close_button: OnEditor<Gd<Button>>,

    #[export]
    toggle_button: OnEditor<Gd<Button>>,

    #[export]
    small_homes_label: OnEditor<Gd<Label>>,

    #[export]
    small_occupancy_label: OnEditor<Gd<Label>>,

    #[export]
    medium_homes_label: OnEditor<Gd<Label>>,

    #[export]
    medium_occupancy_label: OnEditor<Gd<Label>>,

    #[export]
    large_homes_label: OnEditor<Gd<Label>>,

    #[export]
    large_occupancy_label: OnEditor<Gd<Label>>,

    #[export]
    total_homes_label: OnEditor<Gd<Label>>,

    #[export]
    total_occupancy_label: OnEditor<Gd<Label>>,

    #[export]
    homeless_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    cached_overview: Option<HousingOverview>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for HousingPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            close_button: OnEditor::default(),
            toggle_button: OnEditor::default(),
            small_homes_label: OnEditor::default(),
            small_occupancy_label: OnEditor::default(),
            medium_homes_label: OnEditor::default(),
            medium_occupancy_label: OnEditor::default(),
            large_homes_label: OnEditor::default(),
            large_occupancy_label: OnEditor::default(),
            total_homes_label: OnEditor::default(),
            total_occupancy_label: OnEditor::default(),
            homeless_label: OnEditor::default(),
            game_world: OnEditor::default(),
            cached_overview: None,
            base,
        }
    }

    fn ready(&mut self) {
        let close_button = self.close_button.clone();
        close_button
            .signals()
            .pressed()
            .connect_other(self, Self::hide_panel);

        let toggle_button = self.toggle_button.clone();
        toggle_button
            .signals()
            .pressed()
            .connect_other(self, Self::toggle_panel);

        self.base_mut().hide();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh_overview();
        }
    }
}

impl HousingPanel {
    fn hide_panel(&mut self) {
        self.base_mut().hide();
    }

    fn toggle_panel(&mut self) {
        if self.base().is_visible() {
            self.hide_panel();
        } else {
            self.base_mut().show();
            self.refresh_overview();
        }
    }

    fn refresh_overview(&mut self) {
        let overview = {
            let game_world = self.game_world.bind();
            game_world.with_rendered_surface_world(housing_overview)
        };
        if self.cached_overview.as_ref() == Some(&overview) {
            return;
        }

        set_number(&mut self.small_homes_label.clone(), overview.small.homes);
        set_occupancy(
            &mut self.small_occupancy_label.clone(),
            overview.small.occupied,
            overview.small.capacity,
        );
        set_number(&mut self.medium_homes_label.clone(), overview.medium.homes);
        set_occupancy(
            &mut self.medium_occupancy_label.clone(),
            overview.medium.occupied,
            overview.medium.capacity,
        );
        set_number(&mut self.large_homes_label.clone(), overview.large.homes);
        set_occupancy(
            &mut self.large_occupancy_label.clone(),
            overview.large.occupied,
            overview.large.capacity,
        );
        self.total_homes_label
            .clone()
            .set_text(format!("Total homes: {}", overview.total_homes).as_str());
        self.total_occupancy_label.clone().set_text(
            format!(
                "Housing slots: {}/{}",
                overview.total_occupied, overview.total_capacity
            )
            .as_str(),
        );
        self.homeless_label
            .clone()
            .set_text(format!("Homeless colonists: {}", overview.homeless).as_str());
        self.cached_overview = Some(overview);
    }
}

fn housing_overview(world: &bevy_ecs::world::World) -> HousingOverview {
    let snapshot = housing_snapshot(world);
    let mut overview = HousingOverview {
        homeless: snapshot.homeless().len(),
        ..Default::default()
    };

    for house in snapshot.houses() {
        let row = match house.kind() {
            BuildingKind::SmallHouse => &mut overview.small,
            BuildingKind::MediumHouse => &mut overview.medium,
            BuildingKind::LargeHouse => &mut overview.large,
            _ => continue,
        };
        row.homes += 1;
        row.occupied += house.occupied();
        row.capacity += house.capacity();
        overview.total_homes += 1;
        overview.total_occupied += house.occupied();
        overview.total_capacity += house.capacity();
    }

    overview
}

fn set_number(label: &mut Gd<Label>, value: usize) {
    label.set_text(value.to_string().as_str());
}

fn set_occupancy(label: &mut Gd<Label>, occupied: usize, capacity: usize) {
    label.set_text(format!("{occupied}/{capacity}").as_str());
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::buildings::{Building, BuildingBlueprint, BuildingFootprint};
    use game_engine::components::Npc;
    use game_engine::grid::CellCoord;
    use game_engine::housing::{House, HousingAssignment};

    #[test]
    fn overview_groups_completed_houses_and_excludes_blueprints() {
        let mut world = bevy_ecs::world::World::new();
        let small = world
            .spawn((
                Building::new(
                    BuildingKind::SmallHouse,
                    BuildingFootprint::new(CellCoord::new(0, 0), 1, 1),
                ),
                House::new(2, 0),
            ))
            .id();
        world.spawn((
            Building::new(
                BuildingKind::LargeHouse,
                BuildingFootprint::new(CellCoord::new(2, 2), 3, 3),
            ),
            House::new(8, 1),
        ));
        world.spawn(BuildingBlueprint {
            kind: BuildingKind::MediumHouse,
            footprint: BuildingFootprint::new(CellCoord::new(5, 5), 2, 2),
        });
        world.spawn((Npc, HousingAssignment::new(small, 0)));
        world.spawn(Npc);

        let overview = housing_overview(&world);

        assert_eq!(
            overview.small,
            HousingTierRow {
                homes: 1,
                occupied: 1,
                capacity: 2
            }
        );
        assert_eq!(overview.medium, HousingTierRow::default());
        assert_eq!(
            overview.large,
            HousingTierRow {
                homes: 1,
                occupied: 0,
                capacity: 8
            }
        );
        assert_eq!(overview.total_homes, 2);
        assert_eq!(overview.total_occupied, 1);
        assert_eq!(overview.total_capacity, 10);
        assert_eq!(overview.homeless, 1);
    }
}
