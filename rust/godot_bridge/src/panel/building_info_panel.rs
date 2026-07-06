use super::resource_quantity::ResourceQuantity;
use super::resource_quantity_progress::ResourceQuantityProgress;
use crate::world::game_world::{decode_entity_id, GameWorld};
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
    WarehouseInventory,
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
    construction_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    wood_construction_progress: OnEditor<Gd<ResourceQuantityProgress>>,

    #[export]
    stone_construction_progress: OnEditor<Gd<ResourceQuantityProgress>>,

    #[export]
    food_construction_progress: OnEditor<Gd<ResourceQuantityProgress>>,

    #[export]
    gold_construction_progress: OnEditor<Gd<ResourceQuantityProgress>>,

    #[export]
    inventory_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    inventory_label: OnEditor<Gd<Label>>,

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
            construction_container: OnEditor::default(),
            wood_construction_progress: OnEditor::default(),
            stone_construction_progress: OnEditor::default(),
            food_construction_progress: OnEditor::default(),
            gold_construction_progress: OnEditor::default(),
            inventory_container: OnEditor::default(),
            inventory_label: OnEditor::default(),
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
        let construction_container = self.construction_container.clone();
        let wood_construction_progress = self.wood_construction_progress.clone();
        let stone_construction_progress = self.stone_construction_progress.clone();
        let food_construction_progress = self.food_construction_progress.clone();
        let gold_construction_progress = self.gold_construction_progress.clone();
        let inventory_container = self.inventory_container.clone();
        let inventory_label = self.inventory_label.clone();
        let wood_inventory_quantity = self.wood_inventory_quantity.clone();
        let stone_inventory_quantity = self.stone_inventory_quantity.clone();
        let food_inventory_quantity = self.food_inventory_quantity.clone();
        let gold_inventory_quantity = self.gold_inventory_quantity.clone();

        let selected_game_world = game_world.clone();
        let mut selected_name_label = name_label.clone();
        let mut selected_footprint_label = footprint_label.clone();
        let mut selected_construction_container = construction_container.clone();
        let mut selected_wood_construction_progress = wood_construction_progress.clone();
        let mut selected_stone_construction_progress = stone_construction_progress.clone();
        let mut selected_food_construction_progress = food_construction_progress.clone();
        let mut selected_gold_construction_progress = gold_construction_progress.clone();
        let mut selected_inventory_container = inventory_container.clone();
        let mut selected_inventory_label = inventory_label.clone();
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
                        &mut selected_construction_container,
                        &mut selected_inventory_container,
                        &mut selected_inventory_label,
                    );
                    return;
                };

                selected_name_label.set_text(format!("Building: {}", info.kind.label()).as_str());
                selected_footprint_label.set_text(format_footprint(info.footprint).as_str());
                match info.construction {
                    Some(construction) => update_construction_progress(
                        &mut selected_construction_container,
                        &mut selected_wood_construction_progress,
                        &mut selected_stone_construction_progress,
                        &mut selected_food_construction_progress,
                        &mut selected_gold_construction_progress,
                        construction.progress,
                        construction.cost,
                    ),
                    None => update_construction_progress(
                        &mut selected_construction_container,
                        &mut selected_wood_construction_progress,
                        &mut selected_stone_construction_progress,
                        &mut selected_food_construction_progress,
                        &mut selected_gold_construction_progress,
                        ResourceAmounts::zero(),
                        ResourceAmounts::zero(),
                    ),
                };
                update_inventory(
                    &mut selected_inventory_container,
                    &mut selected_inventory_label,
                    &mut selected_wood_inventory_quantity,
                    &mut selected_stone_inventory_quantity,
                    &mut selected_food_inventory_quantity,
                    &mut selected_gold_inventory_quantity,
                    info.inventory,
                );
            });

        let mut deselected_name_label = name_label;
        let mut deselected_footprint_label = footprint_label;
        let mut deselected_construction_container = construction_container;
        let mut deselected_inventory_container = inventory_container;
        let mut deselected_inventory_label = inventory_label;
        game_world.signals().building_deselected().connect(move || {
            clear_building_labels(
                &mut deselected_name_label,
                &mut deselected_footprint_label,
                &mut deselected_construction_container,
                &mut deselected_inventory_container,
                &mut deselected_inventory_label,
            );
        });
    }
}

struct BuildingInfo {
    kind: BuildingKind,
    footprint: BuildingFootprint,
    construction: Option<BuildingConstructionInfo>,
    inventory: Option<WarehouseInventory>,
}

struct BuildingConstructionInfo {
    cost: ResourceAmounts,
    progress: ResourceAmounts,
}

fn building_info(game_world: &GameWorld, building_entity_id: i64) -> Option<BuildingInfo> {
    let entity = decode_entity_id(building_entity_id)?;
    game_world.with_rendered_surface_world(|world| {
        let inventory = world.get::<WarehouseInventory>(entity).copied();

        if let Some(blueprint) = world.get::<BuildingBlueprint>(entity) {
            let progress = world.get::<ConstructionProgress>(entity)?;
            return Some(BuildingInfo {
                kind: blueprint.kind,
                footprint: blueprint.footprint,
                construction: Some(BuildingConstructionInfo {
                    cost: blueprint.kind.definition().construction_cost(),
                    progress: progress.deposited(),
                }),
                inventory,
            });
        }

        let building = world.get::<Building>(entity)?;
        Some(BuildingInfo {
            kind: building.kind,
            footprint: building.footprint,
            construction: None,
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

#[derive(Debug, PartialEq, Eq)]
struct ConstructionProgressRow {
    kind: ResourceKind,
    deposited: u32,
    required: u32,
}

#[cfg(test)]
fn construction_progress_rows(
    progress: ResourceAmounts,
    cost: ResourceAmounts,
) -> Vec<ConstructionProgressRow> {
    ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| construction_progress_row(kind, progress, cost))
        .collect()
}

fn construction_progress_row(
    kind: ResourceKind,
    progress: ResourceAmounts,
    cost: ResourceAmounts,
) -> Option<ConstructionProgressRow> {
    let required = cost.get(kind);
    (required > 0).then(|| ConstructionProgressRow {
        kind,
        deposited: progress.get(kind),
        required,
    })
}

fn update_construction_progress(
    construction_container: &mut Gd<VBoxContainer>,
    wood_progress: &mut Gd<ResourceQuantityProgress>,
    stone_progress: &mut Gd<ResourceQuantityProgress>,
    food_progress: &mut Gd<ResourceQuantityProgress>,
    gold_progress: &mut Gd<ResourceQuantityProgress>,
    progress: ResourceAmounts,
    cost: ResourceAmounts,
) {
    let has_wood =
        update_construction_progress_row(wood_progress, ResourceKind::Wood, progress, cost);
    let has_stone =
        update_construction_progress_row(stone_progress, ResourceKind::Stone, progress, cost);
    let has_food =
        update_construction_progress_row(food_progress, ResourceKind::Food, progress, cost);
    let has_gold =
        update_construction_progress_row(gold_progress, ResourceKind::Gold, progress, cost);

    if has_wood || has_stone || has_food || has_gold {
        construction_container.show();
    } else {
        construction_container.hide();
    }
}

fn update_construction_progress_row(
    progress_node: &mut Gd<ResourceQuantityProgress>,
    kind: ResourceKind,
    progress: ResourceAmounts,
    cost: ResourceAmounts,
) -> bool {
    let Some(row) = construction_progress_row(kind, progress, cost) else {
        progress_node.bind_mut().hide_progress();
        return false;
    };

    let mut progress_node = progress_node.bind_mut();
    progress_node.set_resource_kind(row.kind);
    progress_node.set_amounts(row.deposited, row.required);
    progress_node.show_progress();
    true
}

fn update_inventory(
    inventory_container: &mut Gd<VBoxContainer>,
    inventory_label: &mut Gd<Label>,
    wood_quantity: &mut Gd<ResourceQuantity>,
    stone_quantity: &mut Gd<ResourceQuantity>,
    food_quantity: &mut Gd<ResourceQuantity>,
    gold_quantity: &mut Gd<ResourceQuantity>,
    inventory: Option<WarehouseInventory>,
) {
    if let Some(inventory) = inventory {
        let contents = inventory.contents();
        inventory_label
            .set_text(inventory_header_text(inventory.used_size(), inventory.max_size()).as_str());
        wood_quantity
            .bind_mut()
            .set_amount(contents.get(ResourceKind::Wood));
        stone_quantity
            .bind_mut()
            .set_amount(contents.get(ResourceKind::Stone));
        food_quantity
            .bind_mut()
            .set_amount(contents.get(ResourceKind::Food));
        gold_quantity
            .bind_mut()
            .set_amount(contents.get(ResourceKind::Gold));
        inventory_container.show();
    } else {
        inventory_container.hide();
    }
}

fn clear_building_labels(
    name_label: &mut Gd<Label>,
    footprint_label: &mut Gd<Label>,
    construction_container: &mut Gd<VBoxContainer>,
    inventory_container: &mut Gd<VBoxContainer>,
    inventory_label: &mut Gd<Label>,
) {
    name_label.set_text("Building: None");
    footprint_label.set_text("");
    construction_container.hide();
    inventory_label.set_text("Inventory:");
    inventory_container.hide();
}

fn inventory_header_text(used_size: u32, max_size: u32) -> String {
    format!("Inventory: {used_size}/{max_size}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_progress_rows_show_deposited_over_required() {
        let progress = ResourceAmounts::new(5, 0, 0, 0);
        let cost = ResourceAmounts::new(40, 20, 0, 0);

        assert_eq!(
            construction_progress_rows(progress, cost),
            vec![
                ConstructionProgressRow {
                    kind: ResourceKind::Wood,
                    deposited: 5,
                    required: 40,
                },
                ConstructionProgressRow {
                    kind: ResourceKind::Stone,
                    deposited: 0,
                    required: 20,
                },
            ]
        );
    }

    #[test]
    fn construction_progress_rows_omit_zero_cost_resources() {
        let progress = ResourceAmounts::new(10, 20, 30, 40);
        let cost = ResourceAmounts::new(0, 0, 0, 20);

        assert_eq!(
            construction_progress_rows(progress, cost),
            vec![ConstructionProgressRow {
                kind: ResourceKind::Gold,
                deposited: 40,
                required: 20,
            }]
        );
    }

    #[test]
    fn construction_progress_rows_are_empty_without_required_resources() {
        assert!(
            construction_progress_rows(ResourceAmounts::zero(), ResourceAmounts::zero()).is_empty()
        );
    }

    #[test]
    fn inventory_header_text_shows_used_over_max() {
        assert_eq!(inventory_header_text(125, 2000), "Inventory: 125/2000");
    }
}
