use super::resource_quantity::ResourceQuantity;
use crate::assets::load_packed_scene;
use crate::world::game_world::{decode_entity_id, GameWorld, MapEntityKind};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{
    Building, BuildingActivity, BuildingBlueprint, BuildingKind, BuildingName,
    ConstructionProgress, RefineryPullConfig, StorageInventory, StoragePullConfig,
};
use game_engine::components::{Tile, TilePosition};
use game_engine::housing::housing_snapshot;
use game_engine::npcs::{
    BirthDate, CarriedResource, FoodPouch, Npc, NpcName, NpcPosition, WorldDateTime,
};
use game_engine::refining::{recipes_for_building, RefineryInventory};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::{ResourceAmounts, ResourceKind};
use godot::classes::{
    control, HFlowContainer, IPanelContainer, Label, PackedScene, PanelContainer, RichTextLabel,
    VBoxContainer,
};
use godot::obj::OnEditor;
use godot::prelude::*;

const TOOLTIP_CURSOR_OFFSET: Vector2 = Vector2::new(16.0, 16.0);
const RESOURCE_QUANTITY_SCENE_PATH: &str = "res://panel/resource_quantity.tscn";

#[derive(Debug, Clone, PartialEq, Eq)]
struct TooltipView {
    text: String,
    resource_sections: Vec<ResourceSectionView>,
}

impl TooltipView {
    fn text_only(text: String) -> Self {
        Self {
            text,
            resource_sections: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceSectionView {
    label: String,
    entries: Vec<ResourceEntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceEntryView {
    kind: ResourceKind,
    value: Option<String>,
}

struct ResourceSectionControls {
    label: Gd<Label>,
    row: Gd<HFlowContainer>,
    entries: Vec<ResourceEntryControls>,
    empty_label: Option<Gd<Label>>,
}

struct ResourceEntryControls {
    kind: ResourceKind,
    node: Gd<ResourceQuantity>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct MapEntityTooltipPanel {
    #[export]
    text_label: OnEditor<Gd<RichTextLabel>>,

    #[export]
    resource_sections: OnEditor<Gd<VBoxContainer>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    hovered_target: Option<(MapEntityKind, i64)>,
    cached_view: Option<TooltipView>,
    resource_quantity_scene: Option<Gd<PackedScene>>,
    resource_section_controls: Vec<ResourceSectionControls>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for MapEntityTooltipPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            text_label: OnEditor::default(),
            resource_sections: OnEditor::default(),
            game_world: OnEditor::default(),
            hovered_target: None,
            cached_view: None,
            resource_quantity_scene: None,
            resource_section_controls: Vec::new(),
            base,
        }
    }

    fn ready(&mut self) {
        self.resource_quantity_scene =
            load_packed_scene(RESOURCE_QUANTITY_SCENE_PATH, "MapEntityTooltipPanel");
        let game_world = self.game_world.clone();
        game_world
            .signals()
            .map_entity_hovered()
            .connect_other(self, Self::show_entity_tooltip);
        game_world
            .signals()
            .map_entity_unhovered()
            .connect_other(self, Self::hide_tooltip);

        self.base_mut()
            .set_mouse_filter(control::MouseFilter::IGNORE);
        self.base_mut().hide();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh_tooltip();
            self.position_near_mouse();
        }
    }
}

impl MapEntityTooltipPanel {
    fn show_entity_tooltip(&mut self, kind_value: i64, entity_id: i64) {
        let Some(kind) = MapEntityKind::from_signal_value(kind_value) else {
            self.hide_tooltip();
            return;
        };
        self.hovered_target = Some((kind, entity_id));
        self.cached_view = None;
        self.refresh_tooltip();
        if self.hovered_target.is_none() {
            return;
        }
        self.base_mut().show();
        self.position_near_mouse();
    }

    fn hide_tooltip(&mut self) {
        self.hovered_target = None;
        self.cached_view = None;
        self.base_mut().hide();
    }

    fn refresh_tooltip(&mut self) {
        let Some((kind, entity_id)) = self.hovered_target else {
            return;
        };
        let view = {
            let game_world = self.game_world.bind();
            map_entity_tooltip_view(&game_world, kind, entity_id)
        };
        let Some(view) = view else {
            self.hide_tooltip();
            return;
        };
        if self.cached_view.as_ref() == Some(&view) {
            return;
        }

        self.text_label.clone().parse_bbcode(view.text.as_str());
        self.sync_resource_sections(&view.resource_sections);
        self.cached_view = Some(view);
        self.base_mut().reset_size();
    }

    fn sync_resource_sections(&mut self, sections: &[ResourceSectionView]) {
        let shape_matches =
            self.resource_section_controls.len() == sections.len()
                && self.resource_section_controls.iter().zip(sections).all(
                    |(controls, section)| {
                        controls
                            .entries
                            .iter()
                            .map(|entry| entry.kind)
                            .eq(section.entries.iter().map(|entry| entry.kind))
                            && controls.empty_label.is_some() == section.entries.is_empty()
                    },
                );

        if !shape_matches {
            self.rebuild_resource_sections(sections);
        }

        for (controls, section) in self.resource_section_controls.iter_mut().zip(sections) {
            controls.label.set_text(section.label.as_str());
            for (controls, entry) in controls.entries.iter_mut().zip(&section.entries) {
                let mut node = controls.node.bind_mut();
                if let Some(value) = &entry.value {
                    node.set_display_text(value.as_str());
                } else {
                    node.hide_amount();
                }
            }
        }
    }

    fn rebuild_resource_sections(&mut self, sections: &[ResourceSectionView]) {
        for mut controls in self.resource_section_controls.drain(..) {
            controls.label.queue_free();
            controls.row.queue_free();
        }

        let Some(scene) = self.resource_quantity_scene.clone() else {
            return;
        };
        let mut parent = self.resource_sections.clone();
        for section in sections {
            let mut label = Label::new_alloc();
            label.set_text(section.label.as_str());
            label.set_mouse_filter(control::MouseFilter::IGNORE);
            parent.add_child(&label);

            let mut row = HFlowContainer::new_alloc();
            row.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
            row.set_mouse_filter(control::MouseFilter::IGNORE);
            parent.add_child(&row);

            let mut entries = Vec::with_capacity(section.entries.len());
            let mut empty_label = None;
            if section.entries.is_empty() {
                let mut label = Label::new_alloc();
                label.set_text("None");
                label.set_mouse_filter(control::MouseFilter::IGNORE);
                row.add_child(&label);
                empty_label = Some(label);
            } else {
                for entry in &section.entries {
                    let Some(node) = scene.instantiate() else {
                        godot_error!(
                            "MapEntityTooltipPanel: failed to instantiate resource quantity"
                        );
                        continue;
                    };
                    let Ok(mut node) = node.try_cast::<ResourceQuantity>() else {
                        godot_error!(
                            "MapEntityTooltipPanel: resource quantity scene has unexpected root type"
                        );
                        continue;
                    };
                    row.add_child(&node);
                    {
                        let mut quantity = node.bind_mut();
                        quantity.set_resource_kind(entry.kind);
                        quantity.set_mouse_passthrough();
                        if let Some(value) = &entry.value {
                            quantity.set_display_text(value.as_str());
                        } else {
                            quantity.hide_amount();
                        }
                    }
                    entries.push(ResourceEntryControls {
                        kind: entry.kind,
                        node,
                    });
                }
            }

            self.resource_section_controls
                .push(ResourceSectionControls {
                    label,
                    row,
                    entries,
                    empty_label,
                });
        }
    }

    fn position_near_mouse(&mut self) {
        let Some(parent) = self.base().get_parent_control() else {
            return;
        };
        let parent_size = parent.get_size();
        let mouse_pos = parent.get_local_mouse_position();
        let tooltip_size = {
            let base = self.base();
            let size = base.get_size();
            let minimum = base.get_combined_minimum_size();
            Vector2::new(size.x.max(minimum.x), size.y.max(minimum.y))
        };
        let desired = mouse_pos + TOOLTIP_CURSOR_OFFSET;
        let max_x = (parent_size.x - tooltip_size.x).max(0.0);
        let max_y = (parent_size.y - tooltip_size.y).max(0.0);
        let position = Vector2::new(desired.x.clamp(0.0, max_x), desired.y.clamp(0.0, max_y));

        self.base_mut().set_position(position);
    }
}

fn map_entity_tooltip_view(
    game_world: &GameWorld,
    kind: MapEntityKind,
    entity_id: i64,
) -> Option<TooltipView> {
    let entity = decode_entity_id(entity_id)?;
    game_world.with_rendered_surface_world(|world| match kind {
        MapEntityKind::Building => building_tooltip_view(world, entity),
        MapEntityKind::Npc => npc_tooltip_text(world, entity).map(TooltipView::text_only),
        MapEntityKind::ResourceNode => {
            resource_node_tooltip_text(world, entity).map(TooltipView::text_only)
        }
    })
}

fn building_tooltip_view(world: &World, entity: Entity) -> Option<TooltipView> {
    let name = world.get::<BuildingName>(entity)?;

    if let Some(blueprint) = world.get::<BuildingBlueprint>(entity) {
        let progress = world.get::<ConstructionProgress>(entity)?;

        return Some(building_blueprint_tooltip_view(
            name.as_str(),
            blueprint.kind.label(),
            progress.deposited(),
            blueprint.kind.definition().construction_cost(),
            progress.labor_completed(),
            progress.labor_required(),
        ));
    }

    let building = world.get::<Building>(entity)?;
    if let (Some(inventory), Some(activity), Some(pull_config)) = (
        world.get::<StorageInventory>(entity),
        world.get::<BuildingActivity>(entity),
        world.get::<StoragePullConfig>(entity),
    ) {
        return Some(storage_tooltip_view(
            name.as_str(),
            building.kind.label(),
            *activity,
            *inventory,
            *pull_config,
        ));
    }
    if let (Some(inventory), Some(activity), Some(pull_config)) = (
        world.get::<RefineryInventory>(entity),
        world.get::<BuildingActivity>(entity),
        world.get::<RefineryPullConfig>(entity),
    ) {
        return Some(refinery_tooltip_view(
            name.as_str(),
            building.kind,
            *activity,
            *inventory,
            *pull_config,
        ));
    }

    let occupancy = housing_snapshot(world)
        .house(entity)
        .map(|house| (house.occupied(), house.capacity()));
    Some(TooltipView::text_only(format_finished_building_tooltip(
        name.as_str(),
        building.kind.label(),
        occupancy,
    )))
}

fn npc_tooltip_text(world: &World, entity: Entity) -> Option<String> {
    world.get::<Npc>(entity)?;
    let position = world.get::<NpcPosition>(entity)?;
    let name = world.get::<NpcName>(entity)?;
    let birth_date = world.get::<BirthDate>(entity)?;
    let food_pouch = world.get::<FoodPouch>(entity)?;
    let carried_resource = world.get::<CarriedResource>(entity)?;
    let world_date_time = *world.resource::<WorldDateTime>();

    Some(format_npc_tooltip(
        name.as_str(),
        position.coord,
        world_date_time.age_years_since(*birth_date),
        *food_pouch,
        *carried_resource,
    ))
}

fn resource_node_tooltip_text(world: &World, entity: Entity) -> Option<String> {
    world.get::<Tile>(entity)?;
    world.get::<TilePosition>(entity)?;
    let node = world.get::<ResourceNode>(entity)?;

    Some(format_resource_node_tooltip(*node))
}

fn building_blueprint_tooltip_view(
    name: &str,
    label: &str,
    progress: ResourceAmounts,
    cost: ResourceAmounts,
    labor_completed: u32,
    labor_required: u32,
) -> TooltipView {
    let entries = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let required = cost.get(kind);
            (required > 0).then(|| ResourceEntryView {
                kind,
                value: Some(format!("{}/{required}", progress.get(kind))),
            })
        })
        .collect();

    TooltipView {
        text: format!(
            "[b]{name}[/b]\nBlueprint: {label}\nLabor: {labor_completed}/{labor_required}"
        ),
        resource_sections: vec![ResourceSectionView {
            label: "Materials:".to_string(),
            entries,
        }],
    }
}

fn format_finished_building_tooltip(
    name: &str,
    label: &str,
    occupancy: Option<(usize, usize)>,
) -> String {
    let mut text = format!("[b]{name}[/b]\n{label}");
    if let Some((occupied, capacity)) = occupancy {
        text.push_str(format!("\nOccupancy: {occupied}/{capacity}").as_str());
    }
    text
}

fn storage_tooltip_view(
    name: &str,
    label: &str,
    activity: BuildingActivity,
    inventory: StorageInventory,
    pull_config: StoragePullConfig,
) -> TooltipView {
    let contents = inventory.contents();
    let stock = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let amount = contents.get(kind);
            (amount > 0).then(|| ResourceEntryView {
                kind,
                value: Some(amount.to_string()),
            })
        })
        .collect();
    let allowed = ResourceKind::ALL
        .into_iter()
        .filter(|kind| inventory.is_allowed(*kind))
        .map(|kind| ResourceEntryView { kind, value: None })
        .collect();
    let pulls = StoragePullConfig::SUPPORTED_RESOURCES
        .into_iter()
        .filter(|kind| pull_config.pulls_from_refineries(*kind))
        .map(|kind| ResourceEntryView { kind, value: None })
        .collect();

    TooltipView {
        text: format!(
            "[b]{name}[/b]\n{label}\nStatus: {}\nCapacity: {}/{}",
            activity_label(activity),
            inventory.used_size(),
            inventory.max_size(),
        ),
        resource_sections: vec![
            ResourceSectionView {
                label: "Stock:".to_string(),
                entries: stock,
            },
            ResourceSectionView {
                label: "Allowed Deposits:".to_string(),
                entries: allowed,
            },
            ResourceSectionView {
                label: "Pull from Refineries:".to_string(),
                entries: pulls,
            },
        ],
    }
}

fn refinery_tooltip_view(
    name: &str,
    kind: BuildingKind,
    activity: BuildingActivity,
    inventory: RefineryInventory,
    pull_config: RefineryPullConfig,
) -> TooltipView {
    let recipes = recipes_for_building(kind);
    let inputs = recipes
        .iter()
        .map(|recipe| recipe.definition().input())
        .collect::<Vec<_>>();
    let mut outputs = Vec::new();
    for output in recipes.iter().map(|recipe| recipe.definition().output()) {
        if !outputs.contains(&output) {
            outputs.push(output);
        }
    }

    let input_contents = inventory.input_contents();
    let output_contents = inventory.output_contents();
    let input_parts = inputs
        .iter()
        .map(|kind| ResourceEntryView {
            kind: *kind,
            value: Some(input_contents.get(*kind).to_string()),
        })
        .collect();
    let output_parts = outputs
        .iter()
        .map(|kind| ResourceEntryView {
            kind: *kind,
            value: Some(output_contents.get(*kind).to_string()),
        })
        .collect();
    let pull_parts = inputs
        .iter()
        .filter(|kind| pull_config.pulls_from_storage(**kind))
        .map(|kind| ResourceEntryView {
            kind: *kind,
            value: None,
        })
        .collect();

    TooltipView {
        text: format!(
            "[b]{name}[/b]\n{}\nStatus: {}",
            kind.label(),
            activity_label(activity),
        ),
        resource_sections: vec![
            ResourceSectionView {
                label: "Inputs:".to_string(),
                entries: input_parts,
            },
            ResourceSectionView {
                label: "Outputs:".to_string(),
                entries: output_parts,
            },
            ResourceSectionView {
                label: "Pull from Storage:".to_string(),
                entries: pull_parts,
            },
        ],
    }
}

fn activity_label(activity: BuildingActivity) -> &'static str {
    if activity.is_active() {
        "Active"
    } else {
        "Inactive"
    }
}

fn format_npc_tooltip(
    name: &str,
    coord: game_engine::grid::CellCoord,
    age_years: u32,
    food_pouch: FoodPouch,
    carried_resource: CarriedResource,
) -> String {
    let cargo = carried_resource.stack().map_or_else(
        || "Empty".to_string(),
        |stack| format!("{}: {}/5", stack.kind().label(), stack.amount()),
    );
    format!(
        "[b]{}[/b]\nNPC\nCell: ({}, {})\nAge: {}\nFood Pouch: {}/{}\nCarried Resource: {}",
        name,
        coord.x(),
        coord.y(),
        age_years,
        food_pouch.amount(),
        food_pouch.capacity(),
        cargo,
    )
}

fn format_resource_node_tooltip(node: ResourceNode) -> String {
    format!(
        "[b]{} Resource Node[/b]\nQuantity: {}\n{}",
        node.kind.label(),
        node.quantity,
        node.kind.description()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::grid::CellCoord;

    fn entry(kind: ResourceKind, value: Option<&str>) -> ResourceEntryView {
        ResourceEntryView {
            kind,
            value: value.map(str::to_string),
        }
    }

    #[test]
    fn building_blueprint_tooltip_uses_resource_progress_entries() {
        let view = building_blueprint_tooltip_view(
            "Central Depot",
            "Depot",
            ResourceAmounts::new(5, 0, 0, 0),
            ResourceAmounts::new(20, 10, 0, 0),
            12,
            720,
        );

        assert_eq!(
            view,
            TooltipView {
                text: "[b]Central Depot[/b]\nBlueprint: Depot\nLabor: 12/720".to_string(),
                resource_sections: vec![ResourceSectionView {
                    label: "Materials:".to_string(),
                    entries: vec![
                        entry(ResourceKind::Wood, Some("5/20")),
                        entry(ResourceKind::Stone, Some("0/10")),
                    ],
                }],
            }
        );
    }

    #[test]
    fn finished_building_tooltip_shows_custom_name_and_type() {
        let text = format_finished_building_tooltip("Main Hall", "TownHall", None);

        assert_eq!(text, "[b]Main Hall[/b]\nTownHall");
    }

    #[test]
    fn finished_house_tooltip_shows_occupancy_without_resident_details() {
        let text =
            format_finished_building_tooltip("Home Sweet Home", "Medium House", Some((3, 4)));

        assert_eq!(text, "[b]Home Sweet Home[/b]\nMedium House\nOccupancy: 3/4");
    }

    #[test]
    fn storage_tooltip_uses_quantities_and_enabled_resource_icons() {
        let mut inventory = StorageInventory::for_kind(BuildingKind::Depot);
        assert!(inventory.add(ResourceKind::Wood, 7));
        assert!(inventory.add(ResourceKind::Food, 3));
        inventory.set_allowed(ResourceKind::Stone, false);
        inventory.set_allowed(ResourceKind::Gold, false);
        let mut pulls = StoragePullConfig::default();
        pulls.set_pulls_from_refineries(ResourceKind::Food, true);

        let view = storage_tooltip_view(
            "Supply Depot",
            "Depot",
            BuildingActivity::active(),
            inventory,
            pulls,
        );

        assert_eq!(
            view.text,
            "[b]Supply Depot[/b]\nDepot\nStatus: Active\nCapacity: 10/500"
        );
        assert_eq!(
            view.resource_sections,
            vec![
                ResourceSectionView {
                    label: "Stock:".to_string(),
                    entries: vec![
                        entry(ResourceKind::Wood, Some("7")),
                        entry(ResourceKind::Food, Some("3")),
                    ],
                },
                ResourceSectionView {
                    label: "Allowed Deposits:".to_string(),
                    entries: vec![
                        entry(ResourceKind::Wood, None),
                        entry(ResourceKind::Food, None),
                        entry(ResourceKind::Crops, None),
                        entry(ResourceKind::WildBerries, None),
                        entry(ResourceKind::Planks, None),
                        entry(ResourceKind::StoneBlocks, None),
                    ],
                },
                ResourceSectionView {
                    label: "Pull from Refineries:".to_string(),
                    entries: vec![entry(ResourceKind::Food, None)],
                },
            ]
        );
    }

    #[test]
    fn refinery_tooltip_includes_zero_quantities_and_only_enabled_pulls() {
        let mut inventory = RefineryInventory::empty();
        assert!(inventory.add_input(BuildingKind::Kitchen, ResourceKind::Crops, 4));
        assert!(inventory.add_output(BuildingKind::Kitchen, ResourceKind::Food, 2));
        let mut pulls = RefineryPullConfig::default();
        pulls.set_pulls_from_storage(ResourceKind::WildBerries, true);
        let mut activity = BuildingActivity::active();
        activity.set_active(false);

        let view = refinery_tooltip_view(
            "Community Kitchen",
            BuildingKind::Kitchen,
            activity,
            inventory,
            pulls,
        );

        assert_eq!(
            view,
            TooltipView {
                text: "[b]Community Kitchen[/b]\nKitchen\nStatus: Inactive".to_string(),
                resource_sections: vec![
                    ResourceSectionView {
                        label: "Inputs:".to_string(),
                        entries: vec![
                            entry(ResourceKind::Crops, Some("4")),
                            entry(ResourceKind::WildBerries, Some("0")),
                        ],
                    },
                    ResourceSectionView {
                        label: "Outputs:".to_string(),
                        entries: vec![entry(ResourceKind::Food, Some("2"))],
                    },
                    ResourceSectionView {
                        label: "Pull from Storage:".to_string(),
                        entries: vec![entry(ResourceKind::WildBerries, None)],
                    },
                ],
            }
        );
    }

    #[test]
    fn storage_tooltip_keeps_empty_resource_sections() {
        let inventory = StorageInventory::for_kind(BuildingKind::Depot);

        let view = storage_tooltip_view(
            "Empty Depot",
            "Depot",
            BuildingActivity::active(),
            inventory,
            StoragePullConfig::default(),
        );

        assert!(view.resource_sections[0].entries.is_empty());
        assert!(view.resource_sections[2].entries.is_empty());
    }

    #[test]
    fn npc_tooltip_formats_identity_position_age_and_inventory() {
        let text = format_npc_tooltip(
            "Mara Voss",
            CellCoord::new(8, 9),
            32,
            FoodPouch::new(20),
            CarriedResource::of(ResourceKind::Wood, 2),
        );

        assert_eq!(
            text,
            "[b]Mara Voss[/b]\nNPC\nCell: (8, 9)\nAge: 32\nFood Pouch: 20/100\nCarried Resource: Wood: 2/5"
        );
    }

    #[test]
    fn npc_tooltip_formats_empty_inventory_as_none() {
        let text = format_npc_tooltip(
            "Mara Voss",
            CellCoord::new(8, 9),
            32,
            FoodPouch::empty(),
            CarriedResource::empty(),
        );

        assert_eq!(
            text,
            "[b]Mara Voss[/b]\nNPC\nCell: (8, 9)\nAge: 32\nFood Pouch: 0/100\nCarried Resource: Empty"
        );
    }

    #[test]
    fn resource_node_tooltip_formats_quantity_and_description() {
        let text = format_resource_node_tooltip(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 72,
        });

        assert_eq!(
            text,
            "[b]Wood Resource Node[/b]\nQuantity: 72\nFlexible timber used for basic construction, repairs, and early infrastructure."
        );
    }
}
