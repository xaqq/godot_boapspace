use super::resource_quantity::ResourceQuantity;
use super::resource_quantity_progress::ResourceQuantityProgress;
use crate::assets::{load_packed_scene, load_texture, resource_asset_path};
use crate::world::game_world::{decode_entity_id, GameWorld};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, BuildingKind, ConstructionProgress,
    WarehouseInventory,
};
use game_engine::farming::{
    farm_field_counts, field_crop_state, FarmInventory, FieldCrop, FieldCropState, FieldOwner,
    FIELD_GROWTH_TICKS, FIELD_SEEDING_TICKS,
};
use game_engine::forestry::{
    forester_lodge_tree_plot_counts, tree_plot_state, ForesterLodgeInventory, TreePlotGrowth,
    TreePlotOwner, TreePlotState, TREE_PLOT_GROWTH_TICKS, TREE_PLOT_SEEDING_TICKS,
};
use game_engine::housing::housing_snapshot;
use game_engine::npcs::NpcName;
use game_engine::refining::{refinery_status, RecipeKind, RefineryStatus, REFINING_TICKS_PER_UNIT};
#[cfg(test)]
use game_engine::refining::{RefineryInventory, RefineryProduction};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use godot::classes::{
    control, Button, CheckButton, IPanelContainer, InputEvent, InputEventMouseButton, Label,
    PackedScene, PanelContainer, VBoxContainer,
};
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

const RESOURCE_QUANTITY_SCENE_PATH: &str = "res://panel/resource_quantity.tscn";
const RESOURCE_QUANTITY_PROGRESS_SCENE_PATH: &str = "res://panel/resource_quantity_progress.tscn";

struct QuantityRowControl {
    kind: ResourceKind,
    node: Gd<ResourceQuantity>,
}

struct ConstructionRowControl {
    kind: ResourceKind,
    node: Gd<ResourceQuantityProgress>,
}

struct WarehouseFilterControl {
    kind: ResourceKind,
    button: Gd<CheckButton>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct BuildingInfoPanel {
    #[export]
    close_button: OnEditor<Gd<Button>>,

    #[export]
    name_label: OnEditor<Gd<Label>>,

    #[export]
    footprint_label: OnEditor<Gd<Label>>,

    #[export]
    farming_info_label: OnEditor<Gd<Label>>,

    #[export]
    fields_button: OnEditor<Gd<Button>>,

    #[export]
    tree_plots_button: OnEditor<Gd<Button>>,

    #[export]
    construction_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    construction_rows_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    inventory_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    inventory_label: OnEditor<Gd<Label>>,

    #[export]
    inventory_rows_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    warehouse_filter_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    warehouse_filter_rows: OnEditor<Gd<VBoxContainer>>,

    #[export]
    refinery_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    refinery_input_label: OnEditor<Gd<Label>>,

    #[export]
    refinery_input_rows_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    refinery_output_label: OnEditor<Gd<Label>>,

    #[export]
    refinery_output_rows_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    refinery_recipe_label: OnEditor<Gd<Label>>,

    #[export]
    refinery_progress_label: OnEditor<Gd<Label>>,

    #[export]
    refinery_worker_label: OnEditor<Gd<Label>>,

    #[export]
    refinery_blocked_label: OnEditor<Gd<Label>>,

    #[export]
    housing_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    housing_occupancy_label: OnEditor<Gd<Label>>,

    #[export]
    housing_rows: OnEditor<Gd<VBoxContainer>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    selected_building_entity_id: Option<i64>,
    resource_quantity_scene: Option<Gd<PackedScene>>,
    resource_quantity_progress_scene: Option<Gd<PackedScene>>,
    construction_rows: Vec<ConstructionRowControl>,
    inventory_rows: Vec<QuantityRowControl>,
    refinery_input_rows: Vec<QuantityRowControl>,
    refinery_output_rows: Vec<QuantityRowControl>,
    housing_row_labels: Vec<Gd<Label>>,
    warehouse_filter_controls: Vec<WarehouseFilterControl>,
    cached_housing: Option<Option<HousingInfo>>,
    suppress_opening_mouse_press: bool,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for BuildingInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            close_button: OnEditor::default(),
            name_label: OnEditor::default(),
            footprint_label: OnEditor::default(),
            farming_info_label: OnEditor::default(),
            fields_button: OnEditor::default(),
            tree_plots_button: OnEditor::default(),
            construction_container: OnEditor::default(),
            construction_rows_container: OnEditor::default(),
            inventory_container: OnEditor::default(),
            inventory_label: OnEditor::default(),
            inventory_rows_container: OnEditor::default(),
            warehouse_filter_container: OnEditor::default(),
            warehouse_filter_rows: OnEditor::default(),
            refinery_container: OnEditor::default(),
            refinery_input_label: OnEditor::default(),
            refinery_input_rows_container: OnEditor::default(),
            refinery_output_label: OnEditor::default(),
            refinery_output_rows_container: OnEditor::default(),
            refinery_recipe_label: OnEditor::default(),
            refinery_progress_label: OnEditor::default(),
            refinery_worker_label: OnEditor::default(),
            refinery_blocked_label: OnEditor::default(),
            housing_container: OnEditor::default(),
            housing_occupancy_label: OnEditor::default(),
            housing_rows: OnEditor::default(),
            game_world: OnEditor::default(),
            selected_building_entity_id: None,
            resource_quantity_scene: None,
            resource_quantity_progress_scene: None,
            construction_rows: Vec::new(),
            inventory_rows: Vec::new(),
            refinery_input_rows: Vec::new(),
            refinery_output_rows: Vec::new(),
            housing_row_labels: Vec::new(),
            warehouse_filter_controls: Vec::new(),
            cached_housing: None,
            suppress_opening_mouse_press: false,
            base,
        }
    }

    fn ready(&mut self) {
        self.resource_quantity_scene =
            load_packed_scene(RESOURCE_QUANTITY_SCENE_PATH, "BuildingInfoPanel");
        self.resource_quantity_progress_scene =
            load_packed_scene(RESOURCE_QUANTITY_PROGRESS_SCENE_PATH, "BuildingInfoPanel");

        self.close_button
            .clone()
            .signals()
            .pressed()
            .connect_other(self, Self::close_panel);

        let game_world = self.game_world.clone();
        game_world
            .signals()
            .building_selected()
            .connect_other(self, Self::select_building);

        let game_world = self.game_world.clone();
        game_world
            .signals()
            .building_deselected()
            .connect_other(self, Self::deselect_building);

        let fields_button = self.fields_button.clone();
        let game_world = self.game_world.clone();
        fields_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_field_placement_for_selected_farm();
            },
        );

        let tree_plots_button = self.tree_plots_button.clone();
        let game_world = self.game_world.clone();
        tree_plots_button.signals().pressed().connect_other(
            &game_world,
            |game_world: &mut GameWorld| {
                game_world.start_tree_plot_placement_for_selected_lodge();
            },
        );

        self.clear_selection_and_hide();
        self.build_warehouse_filter_rows();
        self.base_mut().set_process(true);
        self.base_mut().set_process_input(true);
    }

    fn process(&mut self, _delta: f64) {
        self.refresh_selected_building();
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible() {
            return;
        }
        if event.is_action_pressed("menu_toggle") {
            self.close_panel();
            self.mark_input_handled();
            return;
        }
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if self.suppress_opening_mouse_press {
            if !mouse.is_pressed() {
                self.suppress_opening_mouse_press = false;
            }
            return;
        }
        let rect = self.base().get_global_rect();
        let point = mouse.get_position();
        let inside = point.x >= rect.position.x
            && point.y >= rect.position.y
            && point.x <= rect.position.x + rect.size.x
            && point.y <= rect.position.y + rect.size.y;
        if mouse.is_pressed() && !inside {
            self.close_panel();
            self.mark_input_handled();
        }
    }
}

impl BuildingInfoPanel {
    fn select_building(&mut self, building_entity_id: i64) {
        self.selected_building_entity_id = Some(building_entity_id);
        self.suppress_opening_mouse_press = true;
        self.refresh_selected_building();
        let mouse_position = self.base().get_global_mouse_position();
        let panel_size = self.base().get_size();
        let viewport_size = self
            .base()
            .get_viewport()
            .map(|viewport| viewport.get_visible_rect().size)
            .unwrap_or(Vector2::new(1920.0, 1080.0));
        let desired = mouse_position + Vector2::new(8.0, 8.0);
        self.base_mut().set_global_position(Vector2::new(
            desired
                .x
                .clamp(0.0, (viewport_size.x - panel_size.x).max(0.0)),
            desired
                .y
                .clamp(0.0, (viewport_size.y - panel_size.y).max(0.0)),
        ));
    }

    fn deselect_building(&mut self) {
        self.clear_selection_and_hide();
    }

    fn refresh_selected_building(&mut self) {
        let Some(building_entity_id) = self.selected_building_entity_id else {
            return;
        };
        let info = {
            let game_world = self.game_world.bind();
            building_info(&game_world, building_entity_id)
        };

        let Some(info) = info else {
            self.clear_selection_and_hide();
            return;
        };

        self.update_building_labels(info);
        self.base_mut().show();
    }

    fn update_building_labels(&mut self, info: BuildingInfo) {
        let mut name_label = self.name_label.clone();
        let mut footprint_label = self.footprint_label.clone();
        let mut farming_info_label = self.farming_info_label.clone();
        let mut fields_button = self.fields_button.clone();
        let mut tree_plots_button = self.tree_plots_button.clone();

        name_label.set_text(format!("Building: {}", info.kind.label()).as_str());
        footprint_label.set_text(format_footprint(info.footprint).as_str());
        update_farming_info(
            &mut farming_info_label,
            &mut fields_button,
            &mut tree_plots_button,
            info.kind,
            info.farming,
            info.forestry,
        );
        self.update_construction(info.construction);
        self.update_inventory(info.inventory);
        self.update_warehouse_filter(info.warehouse_filter);
        self.update_refinery(info.refinery);
        self.update_housing(info.housing);
    }

    fn clear_selection_and_hide(&mut self) {
        self.selected_building_entity_id = None;
        let mut name_label = self.name_label.clone();
        let mut footprint_label = self.footprint_label.clone();
        let mut farming_info_label = self.farming_info_label.clone();
        let mut fields_button = self.fields_button.clone();
        let mut tree_plots_button = self.tree_plots_button.clone();
        let mut construction_container = self.construction_container.clone();
        let mut inventory_container = self.inventory_container.clone();
        let mut inventory_label = self.inventory_label.clone();
        let mut refinery_container = self.refinery_container.clone();
        let mut housing_container = self.housing_container.clone();
        let mut warehouse_filter_container = self.warehouse_filter_container.clone();

        clear_building_labels(
            &mut name_label,
            &mut footprint_label,
            &mut farming_info_label,
            &mut fields_button,
            &mut tree_plots_button,
            &mut construction_container,
            &mut inventory_container,
            &mut inventory_label,
            &mut refinery_container,
            &mut housing_container,
        );
        self.sync_construction_rows(&[]);
        self.sync_inventory_rows(&[], ResourceAmounts::zero());
        self.sync_refinery_input_rows(&[], ResourceAmounts::zero());
        self.sync_refinery_output_rows(&[], ResourceAmounts::zero());
        self.clear_housing_rows();
        self.cached_housing = None;
        warehouse_filter_container.hide();
        self.base_mut().hide();
    }

    fn close_panel(&mut self) {
        self.game_world.bind_mut().close_building_context();
    }

    fn mark_input_handled(&self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }

    fn build_warehouse_filter_rows(&mut self) {
        let mut container = self.warehouse_filter_rows.clone();
        for kind in ResourceKind::ALL {
            let mut button = CheckButton::new_alloc();
            button.set_text(kind.label());
            button.set_tooltip_text(format!("{}\n{}", kind.label(), kind.description()).as_str());
            button.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
            if let Some(texture) = load_texture(resource_asset_path(kind), "BuildingInfoPanel") {
                button.set_button_icon(&texture);
                button.set_expand_icon(true);
            }
            button
                .signals()
                .toggled()
                .connect_other(self, move |panel, allowed| {
                    panel.set_warehouse_filter(kind, allowed)
                });
            container.add_child(&button);
            self.warehouse_filter_controls
                .push(WarehouseFilterControl { kind, button });
        }
    }

    fn set_warehouse_filter(&mut self, kind: ResourceKind, allowed: bool) {
        let Some(entity_id) = self.selected_building_entity_id else {
            return;
        };
        if !self
            .game_world
            .bind_mut()
            .set_warehouse_resource_allowed(entity_id, kind, allowed)
        {
            let current = self
                .game_world
                .bind()
                .warehouse_resource_allowed(entity_id, kind);
            if let Some(current) = current {
                if let Some(control) = self
                    .warehouse_filter_controls
                    .iter_mut()
                    .find(|control| control.kind == kind)
                {
                    control.button.set_pressed_no_signal(current);
                }
            } else {
                self.close_panel();
            }
        }
    }

    fn update_warehouse_filter(&mut self, filter: Option<Vec<(ResourceKind, bool)>>) {
        let mut container = self.warehouse_filter_container.clone();
        let Some(filter) = filter else {
            container.hide();
            return;
        };
        for control in &mut self.warehouse_filter_controls {
            let allowed = filter
                .iter()
                .find_map(|(kind, allowed)| (*kind == control.kind).then_some(*allowed))
                .unwrap_or(true);
            control.button.set_pressed_no_signal(allowed);
        }
        container.show();
    }

    fn update_construction(&mut self, construction: Option<BuildingConstructionInfo>) {
        let rows = construction.map_or_else(Vec::new, |construction| {
            construction_progress_rows(construction.progress, construction.cost)
        });
        self.sync_construction_rows(&rows);
        if rows.is_empty() {
            self.construction_container.clone().hide();
        } else {
            self.construction_container.clone().show();
        }
    }

    fn update_inventory(&mut self, inventory: Option<BuildingInventoryInfo>) {
        let mut inventory_container = self.inventory_container.clone();
        let mut inventory_label = self.inventory_label.clone();
        if let Some(inventory) = inventory {
            inventory_label
                .set_text(inventory_header_text(inventory.used_size, inventory.max_size).as_str());
            let kinds = relevant_resource_kinds(&inventory.visible_kinds, inventory.contents);
            self.sync_inventory_rows(&kinds, inventory.contents);
            inventory_container.show();
        } else {
            self.sync_inventory_rows(&[], ResourceAmounts::zero());
            inventory_container.hide();
        }
    }

    fn update_refinery(&mut self, refinery: Option<RefineryInfo>) {
        let mut container = self.refinery_container.clone();
        let Some(refinery) = refinery else {
            self.sync_refinery_input_rows(&[], ResourceAmounts::zero());
            self.sync_refinery_output_rows(&[], ResourceAmounts::zero());
            container.hide();
            return;
        };

        let status = refinery.status;
        let input_kinds =
            refinery_buffer_kinds(&status.supported_recipes, true, status.input_contents);
        let output_kinds =
            refinery_buffer_kinds(&status.supported_recipes, false, status.output_contents);
        self.sync_refinery_input_rows(&input_kinds, status.input_contents);
        self.sync_refinery_output_rows(&output_kinds, status.output_contents);

        self.refinery_input_label.clone().set_text(
            format!(
                "Input Buffer: {}/{}",
                status.input_contents.total(),
                status.input_capacity
            )
            .as_str(),
        );
        self.refinery_output_label.clone().set_text(
            format!(
                "Output Buffer: {}/{}",
                status.output_contents.total(),
                status.output_capacity
            )
            .as_str(),
        );
        self.refinery_recipe_label
            .clone()
            .set_text(refinery_recipe_text(&status).as_str());
        self.refinery_progress_label
            .clone()
            .set_text(refinery_progress_text(&status).as_str());
        self.refinery_worker_label.clone().set_text(
            refinery
                .assigned_worker
                .as_deref()
                .map_or("Worker: Unassigned".to_string(), |worker| {
                    format!("Worker: {worker}")
                })
                .as_str(),
        );
        let mut blocked_label = self.refinery_blocked_label.clone();
        if let Some(reason) = status.blocked_reason {
            blocked_label.set_text(format!("Blocked: {}", reason.label()).as_str());
            blocked_label.show();
        } else {
            blocked_label.set_text("");
            blocked_label.hide();
        }
        container.show();
    }

    fn sync_construction_rows(&mut self, rows: &[ConstructionProgressRow]) {
        let Some(scene) = self.resource_quantity_progress_scene.as_ref() else {
            return;
        };
        let mut container = self.construction_rows_container.clone();
        sync_construction_row_controls(scene, &mut container, &mut self.construction_rows, rows);
    }

    fn sync_inventory_rows(&mut self, kinds: &[ResourceKind], contents: ResourceAmounts) {
        let Some(scene) = self.resource_quantity_scene.as_ref() else {
            return;
        };
        let mut container = self.inventory_rows_container.clone();
        sync_quantity_row_controls(
            scene,
            &mut container,
            &mut self.inventory_rows,
            kinds,
            contents,
        );
    }

    fn sync_refinery_input_rows(&mut self, kinds: &[ResourceKind], contents: ResourceAmounts) {
        let Some(scene) = self.resource_quantity_scene.as_ref() else {
            return;
        };
        let mut container = self.refinery_input_rows_container.clone();
        sync_quantity_row_controls(
            scene,
            &mut container,
            &mut self.refinery_input_rows,
            kinds,
            contents,
        );
    }

    fn sync_refinery_output_rows(&mut self, kinds: &[ResourceKind], contents: ResourceAmounts) {
        let Some(scene) = self.resource_quantity_scene.as_ref() else {
            return;
        };
        let mut container = self.refinery_output_rows_container.clone();
        sync_quantity_row_controls(
            scene,
            &mut container,
            &mut self.refinery_output_rows,
            kinds,
            contents,
        );
    }

    fn update_housing(&mut self, housing: Option<HousingInfo>) {
        if self.cached_housing.as_ref() == Some(&housing) {
            return;
        }

        self.clear_housing_rows();
        let mut housing_container = self.housing_container.clone();
        let mut housing_occupancy_label = self.housing_occupancy_label.clone();
        let mut housing_rows = self.housing_rows.clone();
        if let Some(housing) = &housing {
            housing_occupancy_label.set_text(
                format!("Housing: {}/{}", housing.occupied, housing.slots.len()).as_str(),
            );
            for (slot, resident) in housing.slots.iter().enumerate() {
                let mut label = Label::new_alloc();
                label.set_text(format!("Slot {}: {resident}", slot + 1).as_str());
                label.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
                housing_rows.add_child(&label);
                self.housing_row_labels.push(label);
            }
            housing_container.show();
        } else {
            housing_occupancy_label.set_text("");
            housing_container.hide();
        }
        self.cached_housing = Some(housing);
    }

    fn clear_housing_rows(&mut self) {
        for mut label in self.housing_row_labels.drain(..) {
            label.queue_free();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildingInfo {
    kind: BuildingKind,
    footprint: BuildingFootprint,
    construction: Option<BuildingConstructionInfo>,
    inventory: Option<BuildingInventoryInfo>,
    refinery: Option<RefineryInfo>,
    farming: Option<FarmingInfo>,
    forestry: Option<ForestryInfo>,
    housing: Option<HousingInfo>,
    warehouse_filter: Option<Vec<(ResourceKind, bool)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HousingInfo {
    occupied: usize,
    slots: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildingConstructionInfo {
    cost: ResourceAmounts,
    progress: ResourceAmounts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildingInventoryInfo {
    contents: ResourceAmounts,
    used_size: u32,
    max_size: u32,
    visible_kinds: Vec<ResourceKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RefineryInfo {
    status: RefineryStatus,
    assigned_worker: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FarmingInfo {
    Farm {
        linked_fields: usize,
        constructed_fields: usize,
    },
    Field {
        owner: Entity,
        crop: Option<FieldCrop>,
        state: Option<FieldCropState>,
        blocked_by_full_inventory: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForestryInfo {
    ForesterLodge {
        linked_tree_plots: usize,
        constructed_tree_plots: usize,
    },
    TreePlot {
        owner: Entity,
        growth: Option<TreePlotGrowth>,
        state: Option<TreePlotState>,
        blocked_by_full_inventory: bool,
    },
}

fn building_info(game_world: &GameWorld, building_entity_id: i64) -> Option<BuildingInfo> {
    let entity = decode_entity_id(building_entity_id)?;
    game_world.with_rendered_surface_world(|world| building_info_from_world(world, entity))
}

fn building_info_from_world(world: &World, entity: Entity) -> Option<BuildingInfo> {
    let inventory = building_inventory_info(world, entity);

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
            refinery: None,
            farming: farming_info(world, entity, blueprint.kind),
            forestry: forestry_info(world, entity, blueprint.kind),
            housing: None,
            warehouse_filter: None,
        });
    }

    let building = world.get::<Building>(entity)?;
    Some(BuildingInfo {
        kind: building.kind,
        footprint: building.footprint,
        construction: None,
        inventory,
        refinery: refinery_info(world, entity),
        farming: farming_info(world, entity, building.kind),
        forestry: forestry_info(world, entity, building.kind),
        housing: housing_info(world, entity),
        warehouse_filter: world.get::<WarehouseInventory>(entity).map(|inventory| {
            ResourceKind::ALL
                .into_iter()
                .map(|kind| (kind, inventory.is_allowed(kind)))
                .collect()
        }),
    })
}

fn housing_info(world: &World, entity: Entity) -> Option<HousingInfo> {
    let snapshot = housing_snapshot(world);
    let house = snapshot.house(entity)?;
    let slots = house
        .residents()
        .iter()
        .map(|resident| {
            resident.map_or_else(
                || "Vacant".to_string(),
                |resident| {
                    world.get::<NpcName>(resident).map_or_else(
                        || "Unnamed colonist".to_string(),
                        |name| name.as_str().to_string(),
                    )
                },
            )
        })
        .collect();
    Some(HousingInfo {
        occupied: house.occupied(),
        slots,
    })
}

fn building_inventory_info(
    world: &bevy_ecs::world::World,
    entity: Entity,
) -> Option<BuildingInventoryInfo> {
    if let Some(inventory) = world.get::<WarehouseInventory>(entity).copied() {
        return Some(BuildingInventoryInfo {
            contents: inventory.contents(),
            used_size: inventory.used_size(),
            max_size: inventory.max_size(),
            visible_kinds: ResourceKind::ALL.to_vec(),
        });
    }
    if let Some(inventory) = world.get::<FarmInventory>(entity).copied() {
        return Some(BuildingInventoryInfo {
            contents: inventory.contents(),
            used_size: inventory.used_size(),
            max_size: inventory.max_size(),
            visible_kinds: vec![ResourceKind::Crops],
        });
    }
    world
        .get::<ForesterLodgeInventory>(entity)
        .copied()
        .map(|inventory| BuildingInventoryInfo {
            contents: inventory.contents(),
            used_size: inventory.used_size(),
            max_size: inventory.max_size(),
            visible_kinds: vec![ResourceKind::Wood],
        })
}

fn refinery_info(world: &World, entity: Entity) -> Option<RefineryInfo> {
    let status = refinery_status(world, entity)?;
    let assigned_worker = status.assigned_worker.map(|worker| {
        let id = worker.to_bits();
        world.get::<NpcName>(worker).map_or_else(
            || format!("NPC {id}"),
            |name| format!("{} ({id})", name.as_str()),
        )
    });
    Some(RefineryInfo {
        status,
        assigned_worker,
    })
}

fn farming_info(
    world: &bevy_ecs::world::World,
    entity: Entity,
    kind: BuildingKind,
) -> Option<FarmingInfo> {
    match kind {
        BuildingKind::Farm => {
            let (linked_fields, constructed_fields) = farm_field_counts(world, entity);
            Some(FarmingInfo::Farm {
                linked_fields,
                constructed_fields,
            })
        }
        BuildingKind::Field => {
            let owner = world.get::<FieldOwner>(entity)?;
            let crop = world.get::<FieldCrop>(entity).copied();
            let state = field_crop_state(world, entity);
            let blocked_by_full_inventory = state == Some(FieldCropState::Grown)
                && world
                    .get::<FarmInventory>(owner.farm())
                    .is_some_and(|inventory| !inventory.has_crops_capacity());
            Some(FarmingInfo::Field {
                owner: owner.farm(),
                crop,
                state,
                blocked_by_full_inventory,
            })
        }
        _ => None,
    }
}

fn forestry_info(
    world: &bevy_ecs::world::World,
    entity: Entity,
    kind: BuildingKind,
) -> Option<ForestryInfo> {
    match kind {
        BuildingKind::ForesterLodge => {
            let (linked_tree_plots, constructed_tree_plots) =
                forester_lodge_tree_plot_counts(world, entity);
            Some(ForestryInfo::ForesterLodge {
                linked_tree_plots,
                constructed_tree_plots,
            })
        }
        BuildingKind::TreePlot => {
            let owner = world.get::<TreePlotOwner>(entity)?;
            let growth = world.get::<TreePlotGrowth>(entity).copied();
            let state = tree_plot_state(world, entity);
            let blocked_by_full_inventory = state == Some(TreePlotState::Mature)
                && world
                    .get::<ForesterLodgeInventory>(owner.lodge())
                    .is_some_and(|inventory| !inventory.has_wood_capacity());
            Some(ForestryInfo::TreePlot {
                owner: owner.lodge(),
                growth,
                state,
                blocked_by_full_inventory,
            })
        }
        _ => None,
    }
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

fn sync_construction_row_controls(
    scene: &Gd<PackedScene>,
    container: &mut Gd<VBoxContainer>,
    controls: &mut Vec<ConstructionRowControl>,
    rows: &[ConstructionProgressRow],
) {
    if controls.iter().map(|row| row.kind).collect::<Vec<_>>()
        != rows.iter().map(|row| row.kind).collect::<Vec<_>>()
    {
        for mut control in controls.drain(..) {
            control.node.queue_free();
        }
        for row in rows {
            let Some(node) = scene.instantiate() else {
                godot_error!("BuildingInfoPanel: failed to instantiate construction row");
                return;
            };
            let Ok(mut node) = node.try_cast::<ResourceQuantityProgress>() else {
                godot_error!("BuildingInfoPanel: construction row has unexpected root type");
                return;
            };
            node.bind_mut().set_resource_kind(row.kind);
            container.add_child(&node);
            controls.push(ConstructionRowControl {
                kind: row.kind,
                node,
            });
        }
    }
    for (control, row) in controls.iter_mut().zip(rows) {
        control
            .node
            .bind_mut()
            .set_amounts(row.deposited, row.required);
    }
}

fn sync_quantity_row_controls(
    scene: &Gd<PackedScene>,
    container: &mut Gd<VBoxContainer>,
    controls: &mut Vec<QuantityRowControl>,
    kinds: &[ResourceKind],
    contents: ResourceAmounts,
) {
    if controls.iter().map(|row| row.kind).collect::<Vec<_>>() != kinds {
        for mut control in controls.drain(..) {
            control.node.queue_free();
        }
        for kind in kinds {
            let Some(node) = scene.instantiate() else {
                godot_error!("BuildingInfoPanel: failed to instantiate resource row");
                return;
            };
            let Ok(mut node) = node.try_cast::<ResourceQuantity>() else {
                godot_error!("BuildingInfoPanel: resource row has unexpected root type");
                return;
            };
            node.bind_mut().set_resource_kind(*kind);
            container.add_child(&node);
            controls.push(QuantityRowControl { kind: *kind, node });
        }
    }
    for control in controls {
        control
            .node
            .bind_mut()
            .set_amount(contents.get(control.kind));
    }
}

fn relevant_resource_kinds(
    accepted: &[ResourceKind],
    contents: ResourceAmounts,
) -> Vec<ResourceKind> {
    ResourceKind::ALL
        .into_iter()
        .filter(|kind| accepted.contains(kind) || contents.get(*kind) > 0)
        .collect()
}

fn refinery_buffer_kinds(
    recipes: &[RecipeKind],
    input: bool,
    contents: ResourceAmounts,
) -> Vec<ResourceKind> {
    let accepted = recipes
        .iter()
        .map(|recipe| {
            let definition = recipe.definition();
            if input {
                definition.input()
            } else {
                definition.output()
            }
        })
        .collect::<Vec<_>>();
    relevant_resource_kinds(&accepted, contents)
}

fn refinery_recipe_text(status: &RefineryStatus) -> String {
    status.current_recipe.map_or_else(
        || {
            format!(
                "Recipes: {}",
                status
                    .supported_recipes
                    .iter()
                    .map(|recipe| recipe.label())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        },
        |recipe| format!("Recipe: {}", recipe.label()),
    )
}

fn refinery_progress_text(status: &RefineryStatus) -> String {
    if status.current_recipe.is_none() {
        return "Progress: —".to_string();
    }
    format!(
        "Progress: {}/{} ({} ticks remaining)",
        status.progress_ticks, REFINING_TICKS_PER_UNIT, status.remaining_ticks
    )
}

fn update_farming_info(
    farming_info_label: &mut Gd<Label>,
    fields_button: &mut Gd<Button>,
    tree_plots_button: &mut Gd<Button>,
    kind: BuildingKind,
    farming: Option<FarmingInfo>,
    forestry: Option<ForestryInfo>,
) {
    let info_text = farming
        .map(format_farming_info)
        .or_else(|| forestry.map(format_forestry_info));
    if let Some(info_text) = info_text {
        farming_info_label.set_text(info_text.as_str());
        farming_info_label.show();
    } else {
        farming_info_label.set_text("");
        farming_info_label.hide();
    }

    if kind == BuildingKind::Farm {
        fields_button.show();
    } else {
        fields_button.hide();
    }

    if kind == BuildingKind::ForesterLodge {
        tree_plots_button.show();
    } else {
        tree_plots_button.hide();
    }
}

fn format_farming_info(info: FarmingInfo) -> String {
    match info {
        FarmingInfo::Farm {
            linked_fields,
            constructed_fields,
        } => format!("Fields: {constructed_fields}/{linked_fields} constructed"),
        FarmingInfo::Field {
            owner,
            crop,
            state,
            blocked_by_full_inventory,
        } => {
            let owner_id = owner.to_bits();
            let mut lines = vec![format!("Owner Farm: {owner_id}")];
            match (crop, state) {
                (Some(crop), Some(state)) => {
                    lines.push(format!("Crop: {}", state.label()));
                    lines.push(format!(
                        "Seeding: {}/{}",
                        crop.seeding_progress_ticks(),
                        FIELD_SEEDING_TICKS
                    ));
                    if let Some(growth) = crop.growth_ticks() {
                        lines.push(format!("Growth: {growth}/{FIELD_GROWTH_TICKS}"));
                    }
                }
                _ => lines.push("Crop: Pending construction".to_string()),
            }
            if blocked_by_full_inventory {
                lines.push("Blocked: Farm inventory full".to_string());
            }
            lines.join("\n")
        }
    }
}

fn format_forestry_info(info: ForestryInfo) -> String {
    match info {
        ForestryInfo::ForesterLodge {
            linked_tree_plots,
            constructed_tree_plots,
        } => format!("Tree Plots: {constructed_tree_plots}/{linked_tree_plots} constructed"),
        ForestryInfo::TreePlot {
            owner,
            growth,
            state,
            blocked_by_full_inventory,
        } => {
            let owner_id = owner.to_bits();
            let mut lines = vec![format!("Owner Forester's Lodge: {owner_id}")];
            match (growth, state) {
                (Some(growth), Some(state)) => {
                    lines.push(format!("Tree: {}", state.label()));
                    lines.push(format!(
                        "Seeding: {}/{}",
                        growth.seeding_progress_ticks(),
                        TREE_PLOT_SEEDING_TICKS
                    ));
                    if let Some(growth_ticks) = growth.growth_ticks() {
                        lines.push(format!("Growth: {growth_ticks}/{TREE_PLOT_GROWTH_TICKS}"));
                    }
                }
                _ => lines.push("Tree: Pending construction".to_string()),
            }
            if blocked_by_full_inventory {
                lines.push("Blocked: Forester's Lodge inventory full".to_string());
            }
            lines.join("\n")
        }
    }
}

fn clear_building_labels(
    name_label: &mut Gd<Label>,
    footprint_label: &mut Gd<Label>,
    farming_info_label: &mut Gd<Label>,
    fields_button: &mut Gd<Button>,
    tree_plots_button: &mut Gd<Button>,
    construction_container: &mut Gd<VBoxContainer>,
    inventory_container: &mut Gd<VBoxContainer>,
    inventory_label: &mut Gd<Label>,
    refinery_container: &mut Gd<VBoxContainer>,
    housing_container: &mut Gd<VBoxContainer>,
) {
    name_label.set_text("Building: None");
    footprint_label.set_text("");
    farming_info_label.set_text("");
    farming_info_label.hide();
    fields_button.hide();
    tree_plots_button.hide();
    construction_container.hide();
    inventory_label.set_text("Inventory:");
    inventory_container.hide();
    refinery_container.hide();
    housing_container.hide();
}

fn inventory_header_text(used_size: u32, max_size: u32) -> String {
    format!("Inventory: {used_size}/{max_size}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::grid::CellCoord;
    use game_engine::housing::{House, HousingAssignment};
    use game_engine::npcs::Npc;

    fn footprint(kind: BuildingKind) -> BuildingFootprint {
        let definition = kind.definition();
        BuildingFootprint::new(
            CellCoord::new(1, 2),
            definition.width(),
            definition.height(),
        )
    }

    #[test]
    fn construction_progress_rows_show_deposited_over_required() {
        let progress = ResourceAmounts::of(ResourceKind::Planks, 5);
        let cost = ResourceAmounts::zero()
            .with(ResourceKind::Planks, 40)
            .with(ResourceKind::StoneBlocks, 20);

        assert_eq!(
            construction_progress_rows(progress, cost),
            vec![
                ConstructionProgressRow {
                    kind: ResourceKind::Planks,
                    deposited: 5,
                    required: 40,
                },
                ConstructionProgressRow {
                    kind: ResourceKind::StoneBlocks,
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

    #[test]
    fn building_info_reads_current_warehouse_inventory() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Building::new(BuildingKind::Warehouse, footprint(BuildingKind::Warehouse)),
                WarehouseInventory::empty(),
            ))
            .id();

        assert_eq!(
            building_info_from_world(&world, entity)
                .expect("warehouse info should exist")
                .inventory
                .expect("warehouse should have inventory")
                .contents,
            ResourceAmounts::zero()
        );

        assert!(world
            .get_mut::<WarehouseInventory>(entity)
            .expect("warehouse should have inventory")
            .add(ResourceKind::Wood, 7));

        let inventory = building_info_from_world(&world, entity)
            .expect("warehouse info should still exist")
            .inventory
            .expect("warehouse should still have inventory");
        assert_eq!(inventory.contents, ResourceAmounts::new(7, 0, 0, 0));
        assert_eq!(inventory.used_size, 7);

        world
            .get_mut::<WarehouseInventory>(entity)
            .unwrap()
            .set_allowed(ResourceKind::Stone, false);
        let filter = building_info_from_world(&world, entity)
            .unwrap()
            .warehouse_filter
            .expect("completed warehouse should expose its filter");
        assert_eq!(
            filter
                .iter()
                .find_map(|(kind, allowed)| (*kind == ResourceKind::Stone).then_some(*allowed)),
            Some(false)
        );
    }

    #[test]
    fn building_info_reads_current_farm_inventory() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Building::new(BuildingKind::Farm, footprint(BuildingKind::Farm)),
                FarmInventory::empty(),
            ))
            .id();

        assert_eq!(
            building_info_from_world(&world, entity)
                .expect("farm info should exist")
                .inventory
                .expect("farm should have inventory")
                .contents,
            ResourceAmounts::zero()
        );

        assert!(world
            .get_mut::<FarmInventory>(entity)
            .expect("farm should have inventory")
            .add_crops(5));

        let inventory = building_info_from_world(&world, entity)
            .expect("farm info should still exist")
            .inventory
            .expect("farm should still have inventory");
        assert_eq!(
            inventory.contents,
            ResourceAmounts::of(ResourceKind::Crops, 5)
        );
        assert_eq!(inventory.used_size, 5);
    }

    #[test]
    fn building_info_reads_current_forester_lodge_inventory() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Building::new(
                    BuildingKind::ForesterLodge,
                    footprint(BuildingKind::ForesterLodge),
                ),
                ForesterLodgeInventory::empty(),
            ))
            .id();

        assert!(world
            .get_mut::<ForesterLodgeInventory>(entity)
            .expect("forester lodge should have inventory")
            .add_wood(5));

        let info =
            building_info_from_world(&world, entity).expect("forester lodge info should exist");
        let inventory = info
            .inventory
            .expect("forester lodge should expose inventory");
        assert_eq!(inventory.contents, ResourceAmounts::new(5, 0, 0, 0));
        assert_eq!(inventory.used_size, 5);
        assert_eq!(inventory.max_size, 200);
        assert_eq!(
            info.forestry,
            Some(ForestryInfo::ForesterLodge {
                linked_tree_plots: 0,
                constructed_tree_plots: 0,
            })
        );
    }

    #[test]
    fn building_info_reads_current_construction_progress() {
        let mut world = World::new();
        let kind = BuildingKind::Warehouse;
        let cost = kind.definition().construction_cost();
        let entity = world
            .spawn((
                BuildingBlueprint {
                    kind,
                    footprint: footprint(kind),
                },
                ConstructionProgress::new(ResourceAmounts::zero()),
            ))
            .id();

        assert_eq!(
            building_info_from_world(&world, entity)
                .expect("blueprint info should exist")
                .construction
                .expect("blueprint should have construction info")
                .progress,
            ResourceAmounts::zero()
        );

        assert_eq!(
            world
                .get_mut::<ConstructionProgress>(entity)
                .expect("blueprint should have construction progress")
                .deposit(ResourceKind::Planks, 9, cost),
            9
        );

        assert_eq!(
            building_info_from_world(&world, entity)
                .expect("blueprint info should still exist")
                .construction
                .expect("blueprint should still have construction info")
                .progress,
            ResourceAmounts::of(ResourceKind::Planks, 9)
        );
    }

    #[test]
    fn building_info_exposes_engine_refinery_status() {
        let mut world = World::new();
        let refinery = world
            .spawn((
                Building::new(BuildingKind::Kitchen, footprint(BuildingKind::Kitchen)),
                RefineryInventory::empty(),
                RefineryProduction::default(),
            ))
            .id();

        let info = building_info_from_world(&world, refinery).expect("refinery should be visible");
        let refinery = info.refinery.expect("refinery status should be present");
        assert_eq!(
            refinery.status.supported_recipes,
            vec![RecipeKind::CookCrops, RecipeKind::CookWildBerries]
        );
        assert_eq!(refinery.status.input_capacity, 100);
        assert_eq!(refinery.status.output_capacity, 100);
        assert_eq!(
            refinery_recipe_text(&refinery.status),
            "Recipes: Crops → Food; Wild Berries → Food"
        );
        assert_eq!(refinery_progress_text(&refinery.status), "Progress: —");
    }

    #[test]
    fn relevant_inventory_rows_keep_accepted_zeroes_and_stored_resources() {
        let contents = ResourceAmounts::of(ResourceKind::Gold, 2);
        assert_eq!(
            relevant_resource_kinds(&[ResourceKind::Crops], contents),
            vec![ResourceKind::Gold, ResourceKind::Crops]
        );
    }

    #[test]
    fn refinery_formatting_shows_active_recipe_progress_and_buffer_kinds() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let status = RefineryStatus {
            entity,
            building_kind: BuildingKind::Kitchen,
            input_contents: ResourceAmounts::of(ResourceKind::WildBerries, 3),
            input_capacity: 100,
            output_contents: ResourceAmounts::of(ResourceKind::Food, 2),
            output_capacity: 100,
            supported_recipes: vec![RecipeKind::CookCrops, RecipeKind::CookWildBerries],
            current_recipe: Some(RecipeKind::CookWildBerries),
            progress_ticks: 42,
            remaining_ticks: 18,
            assigned_worker: None,
            blocked_reason: Some(game_engine::refining::RefineryBlockedReason::OutputFull),
        };

        assert_eq!(refinery_recipe_text(&status), "Recipe: Wild Berries → Food");
        assert_eq!(
            refinery_progress_text(&status),
            "Progress: 42/60 (18 ticks remaining)"
        );
        assert_eq!(
            refinery_buffer_kinds(&status.supported_recipes, true, status.input_contents),
            vec![ResourceKind::Crops, ResourceKind::WildBerries]
        );
        assert_eq!(
            refinery_buffer_kinds(&status.supported_recipes, false, status.output_contents),
            vec![ResourceKind::Food]
        );
    }

    #[test]
    fn completed_house_info_lists_numbered_resident_slots() {
        let mut world = World::new();
        let house = world
            .spawn((
                Building::new(
                    BuildingKind::SmallHouse,
                    footprint(BuildingKind::SmallHouse),
                ),
                House::new(2, 0),
            ))
            .id();
        let resident = world.spawn((Npc, NpcName::new("Mara Voss"))).id();
        world
            .entity_mut(resident)
            .insert(HousingAssignment::new(house, 1));

        assert_eq!(
            building_info_from_world(&world, house)
                .expect("house info should exist")
                .housing,
            Some(HousingInfo {
                occupied: 1,
                slots: vec!["Vacant".to_string(), "Mara Voss".to_string()],
            })
        );
    }

    #[test]
    fn house_blueprint_has_no_resident_section() {
        let mut world = World::new();
        let kind = BuildingKind::SmallHouse;
        let blueprint = world
            .spawn((
                BuildingBlueprint {
                    kind,
                    footprint: footprint(kind),
                },
                ConstructionProgress::new(ResourceAmounts::zero()),
            ))
            .id();

        assert_eq!(
            building_info_from_world(&world, blueprint)
                .expect("blueprint info should exist")
                .housing,
            None
        );
    }

    #[test]
    fn building_info_is_none_for_non_building_entity() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        assert_eq!(building_info_from_world(&world, entity), None);
    }

    #[test]
    fn farming_info_text_shows_farm_field_counts() {
        assert_eq!(
            format_farming_info(FarmingInfo::Farm {
                linked_fields: 12,
                constructed_fields: 7,
            }),
            "Fields: 7/12 constructed"
        );
    }

    #[test]
    fn farming_info_text_shows_field_crop_progress_and_full_block() {
        let mut world = bevy_ecs::world::World::new();
        let owner = world.spawn_empty().id();

        assert_eq!(
            format_farming_info(FarmingInfo::Field {
                owner,
                crop: Some(FieldCrop::with_seeding_progress(42)),
                state: Some(FieldCropState::Seeding),
                blocked_by_full_inventory: true,
            }),
            format!(
                "Owner Farm: {}\nCrop: Seeding\nSeeding: 42/{}\nBlocked: Farm inventory full",
                owner.to_bits(),
                FIELD_SEEDING_TICKS
            )
        );
    }

    #[test]
    fn forestry_info_text_shows_lodge_tree_plot_counts() {
        assert_eq!(
            format_forestry_info(ForestryInfo::ForesterLodge {
                linked_tree_plots: 12,
                constructed_tree_plots: 7,
            }),
            "Tree Plots: 7/12 constructed"
        );
    }

    #[test]
    fn forestry_info_text_shows_tree_progress_and_full_block() {
        let mut world = World::new();
        let owner = world.spawn_empty().id();

        assert_eq!(
            format_forestry_info(ForestryInfo::TreePlot {
                owner,
                growth: Some(TreePlotGrowth::with_seeding_progress(42)),
                state: Some(TreePlotState::Seeding),
                blocked_by_full_inventory: true,
            }),
            format!(
                "Owner Forester's Lodge: {}\nTree: Seeding\nSeeding: 42/{}\nBlocked: Forester's Lodge inventory full",
                owner.to_bits(),
                TREE_PLOT_SEEDING_TICKS
            )
        );
    }
}
