use super::resource_quantity::ResourceQuantity;
use crate::world::game_world::{decode_entity_id, GameWorld};
use game_engine::buildings::{
    Building, BuildingBlueprintKind, BuildingFootprint, ConstructionProgress, WarehouseInventory,
};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use godot::classes::{IPanelContainer, Label, PanelContainer, VBoxContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct BuildingInfoPanel {
    #[export]
    name_label: OnEditor<Gd<Label>>,

    #[export]
    footprint_label: OnEditor<Gd<Label>>,

    #[export]
    cost_label: OnEditor<Gd<Label>>,

    #[export]
    progress_label: OnEditor<Gd<Label>>,

    #[export]
    inventory_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    wood_inventory_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    stone_inventory_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    food_inventory_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    gold_inventory_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for BuildingInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            name_label: OnEditor::default(),
            footprint_label: OnEditor::default(),
            cost_label: OnEditor::default(),
            progress_label: OnEditor::default(),
            inventory_container: OnEditor::default(),
            wood_inventory_quantity: OnEditor::default(),
            stone_inventory_quantity: OnEditor::default(),
            food_inventory_quantity: OnEditor::default(),
            gold_inventory_quantity: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        let name_label = self.name_label.clone();
        let footprint_label = self.footprint_label.clone();
        let cost_label = self.cost_label.clone();
        let progress_label = self.progress_label.clone();
        let inventory_container = self.inventory_container.clone();
        let wood_inventory_quantity = self.wood_inventory_quantity.clone();
        let stone_inventory_quantity = self.stone_inventory_quantity.clone();
        let food_inventory_quantity = self.food_inventory_quantity.clone();
        let gold_inventory_quantity = self.gold_inventory_quantity.clone();

        let selected_game_world = game_world.clone();
        let mut selected_name_label = name_label.clone();
        let mut selected_footprint_label = footprint_label.clone();
        let mut selected_cost_label = cost_label.clone();
        let mut selected_progress_label = progress_label.clone();
        let mut selected_inventory_container = inventory_container.clone();
        let mut selected_wood_inventory_quantity = wood_inventory_quantity.clone();
        let mut selected_stone_inventory_quantity = stone_inventory_quantity.clone();
        let mut selected_food_inventory_quantity = food_inventory_quantity.clone();
        let mut selected_gold_inventory_quantity = gold_inventory_quantity.clone();
        game_world
            .signals()
            .building_selected()
            .connect(move |building_entity_id| {
                let game_world = selected_game_world.bind();
                let Some(info) = building_info(&game_world, building_entity_id) else {
                    clear_building_labels(
                        &mut selected_name_label,
                        &mut selected_footprint_label,
                        &mut selected_cost_label,
                        &mut selected_progress_label,
                        &mut selected_inventory_container,
                    );
                    return;
                };

                selected_name_label.set_text(format!("Building: {}", info.kind.label()).as_str());
                selected_footprint_label.set_text(format_footprint(info.footprint).as_str());
                selected_cost_label.set_text(
                    format!("Cost: {}", format_deposited_cost(info.progress, info.cost)).as_str(),
                );
                selected_progress_label.set_text("");
                update_inventory(
                    &mut selected_inventory_container,
                    &mut selected_wood_inventory_quantity,
                    &mut selected_stone_inventory_quantity,
                    &mut selected_food_inventory_quantity,
                    &mut selected_gold_inventory_quantity,
                    info.inventory,
                );
            });

        let mut deselected_name_label = name_label;
        let mut deselected_footprint_label = footprint_label;
        let mut deselected_cost_label = cost_label;
        let mut deselected_progress_label = progress_label;
        let mut deselected_inventory_container = inventory_container;
        game_world.signals().building_deselected().connect(move || {
            clear_building_labels(
                &mut deselected_name_label,
                &mut deselected_footprint_label,
                &mut deselected_cost_label,
                &mut deselected_progress_label,
                &mut deselected_inventory_container,
            );
        });
    }
}

struct BuildingInfo {
    kind: BuildingBlueprintKind,
    footprint: BuildingFootprint,
    cost: ResourceAmounts,
    progress: ResourceAmounts,
    inventory: Option<ResourceAmounts>,
}

fn building_info(game_world: &GameWorld, building_entity_id: i64) -> Option<BuildingInfo> {
    let entity = decode_entity_id(building_entity_id)?;
    game_world.with_rendered_surface_world(|world| {
        let building = world.get::<Building>(entity)?;
        let footprint = world.get::<BuildingFootprint>(entity)?;
        let progress = world.get::<ConstructionProgress>(entity)?;
        let inventory = world
            .get::<WarehouseInventory>(entity)
            .map(|inventory| inventory.contents());

        Some(BuildingInfo {
            kind: building.kind,
            footprint: *footprint,
            cost: building.kind.definition().construction_cost(),
            progress: progress.deposited(),
            inventory,
        })
    })
}

fn format_footprint(footprint: BuildingFootprint) -> String {
    let origin = footprint.origin();
    format!(
        "Footprint: {}x{} at ({}, {})",
        footprint.width(),
        footprint.height(),
        origin.x(),
        origin.y()
    )
}

fn format_deposited_cost(progress: ResourceAmounts, cost: ResourceAmounts) -> String {
    let parts = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let required = cost.get(kind);
            (required > 0).then(|| format!("{}: {}/{}", kind.label(), progress.get(kind), required))
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(", ")
    }
}

fn update_inventory(
    inventory_container: &mut Gd<VBoxContainer>,
    wood_quantity: &mut Gd<ResourceQuantity>,
    stone_quantity: &mut Gd<ResourceQuantity>,
    food_quantity: &mut Gd<ResourceQuantity>,
    gold_quantity: &mut Gd<ResourceQuantity>,
    inventory: Option<ResourceAmounts>,
) {
    if let Some(inventory) = inventory {
        wood_quantity
            .bind_mut()
            .set_amount(inventory.get(ResourceKind::Wood));
        stone_quantity
            .bind_mut()
            .set_amount(inventory.get(ResourceKind::Stone));
        food_quantity
            .bind_mut()
            .set_amount(inventory.get(ResourceKind::Food));
        gold_quantity
            .bind_mut()
            .set_amount(inventory.get(ResourceKind::Gold));
        inventory_container.show();
    } else {
        inventory_container.hide();
    }
}

fn clear_building_labels(
    name_label: &mut Gd<Label>,
    footprint_label: &mut Gd<Label>,
    cost_label: &mut Gd<Label>,
    progress_label: &mut Gd<Label>,
    inventory_container: &mut Gd<VBoxContainer>,
) {
    name_label.set_text("Building: None");
    footprint_label.set_text("");
    cost_label.set_text("");
    progress_label.set_text("");
    inventory_container.hide();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_deposited_cost_shows_deposited_over_required() {
        let progress = ResourceAmounts::new(5, 0, 0, 0);
        let cost = ResourceAmounts::new(40, 20, 0, 0);

        assert_eq!(
            format_deposited_cost(progress, cost),
            "Wood: 5/40, Stone: 0/20"
        );
    }

    #[test]
    fn format_deposited_cost_omits_zero_cost_resources() {
        let progress = ResourceAmounts::new(10, 20, 30, 40);
        let cost = ResourceAmounts::new(0, 0, 0, 20);

        assert_eq!(format_deposited_cost(progress, cost), "Gold: 40/20");
    }

    #[test]
    fn format_deposited_cost_reports_none_without_required_resources() {
        assert_eq!(
            format_deposited_cost(ResourceAmounts::zero(), ResourceAmounts::zero()),
            "None"
        );
    }
}
