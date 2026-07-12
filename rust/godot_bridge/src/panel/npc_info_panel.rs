use super::npc_details::{
    configure_satiation_progress_bar, details_button_enabled, npc_details,
    npc_resource_header_text, update_satiation, NpcDetails,
};
use super::resource_quantity::ResourceQuantity;
use crate::assets::load_packed_scene;
use crate::entity_id::BridgeEntityId;
use crate::world::game_world::GameWorld;
use game_engine::npcs::NpcHunger;
use game_engine::resources::ResourceKind;
use godot::classes::{
    Button, IPanelContainer, Label, PackedScene, PanelContainer, ProgressBar, VBoxContainer,
};
use godot::obj::OnEditor;
use godot::prelude::*;

const RESOURCE_QUANTITY_SCENE_PATH: &str = "res://panel/resource_quantity.tscn";

struct InventoryRowControl {
    kind: ResourceKind,
    node: Gd<ResourceQuantity>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct NpcInfoPanel {
    #[export]
    name_label: OnEditor<Gd<Label>>,

    #[export]
    age_label: OnEditor<Gd<Label>>,

    #[export]
    birth_day_label: OnEditor<Gd<Label>>,

    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    hunger_label: OnEditor<Gd<Label>>,

    #[export]
    satiation_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    satiation_progress_bar: OnEditor<Gd<ProgressBar>>,

    #[export]
    inventory_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    inventory_label: OnEditor<Gd<Label>>,

    #[export]
    inventory_rows_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    details_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    selected_npc_entity_id: Option<BridgeEntityId>,
    resource_quantity_scene: Option<Gd<PackedScene>>,
    inventory_rows: Vec<InventoryRowControl>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for NpcInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            name_label: OnEditor::default(),
            age_label: OnEditor::default(),
            birth_day_label: OnEditor::default(),
            pos_label: OnEditor::default(),
            hunger_label: OnEditor::default(),
            satiation_container: OnEditor::default(),
            satiation_progress_bar: OnEditor::default(),
            inventory_container: OnEditor::default(),
            inventory_label: OnEditor::default(),
            inventory_rows_container: OnEditor::default(),
            details_button: OnEditor::default(),
            game_world: OnEditor::default(),
            selected_npc_entity_id: None,
            resource_quantity_scene: None,
            inventory_rows: Vec::new(),
            base,
        }
    }

    fn ready(&mut self) {
        self.resource_quantity_scene =
            load_packed_scene(RESOURCE_QUANTITY_SCENE_PATH, "NpcInfoPanel");

        let game_world = self.game_world.clone();
        game_world
            .signals()
            .npc_selected()
            .connect_other(self, Self::select_npc);

        let game_world = self.game_world.clone();
        game_world
            .signals()
            .npc_deselected()
            .connect_other(self, Self::deselect_npc);

        let mut satiation_progress_bar = self.satiation_progress_bar.clone();
        configure_satiation_progress_bar(
            &mut satiation_progress_bar,
            NpcHunger::MAX_SATIATION_LEVEL,
        );
        self.clear_and_hide();

        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        self.refresh_selected_npc();
    }
}

impl NpcInfoPanel {
    fn select_npc(&mut self, npc_entity_id: i64) {
        let Ok(npc_entity_id) = BridgeEntityId::try_from(npc_entity_id) else {
            self.clear_and_hide();
            return;
        };
        self.selected_npc_entity_id = Some(npc_entity_id);
        self.refresh_selected_npc();
    }

    fn deselect_npc(&mut self) {
        self.clear_and_hide();
    }

    fn refresh_selected_npc(&mut self) {
        let Some(npc_entity_id) = self.selected_npc_entity_id else {
            return;
        };
        let info = {
            let game_world = self.game_world.bind();
            npc_details(&game_world, npc_entity_id)
        };

        let Some(info) = info else {
            self.clear_and_hide();
            return;
        };

        self.update_npc_labels(info);
        self.set_details_button_enabled(true);
        self.base_mut().show();
    }

    fn update_npc_labels(&mut self, info: NpcDetails) {
        let mut name_label = self.name_label.clone();
        let mut age_label = self.age_label.clone();
        let mut birth_day_label = self.birth_day_label.clone();
        let mut pos_label = self.pos_label.clone();
        let mut hunger_label = self.hunger_label.clone();
        let mut satiation_container = self.satiation_container.clone();
        let mut satiation_progress_bar = self.satiation_progress_bar.clone();
        let mut inventory_container = self.inventory_container.clone();
        let mut inventory_label = self.inventory_label.clone();

        let name_text = format!("Name: {}", info.name);
        let age_text = format!("Age: {}", info.age_years);
        let birth_day_text = format!("Birth Day: {}", info.birth_day);
        let position_text = format!("Cell: ({}, {})", info.coord.x(), info.coord.y());
        name_label.set_text(name_text.as_str());
        age_label.set_text(age_text.as_str());
        birth_day_label.set_text(birth_day_text.as_str());
        pos_label.set_text(position_text.as_str());
        update_satiation(
            &mut hunger_label,
            &mut satiation_container,
            &mut satiation_progress_bar,
            info.hunger_state,
            info.satiation_level,
            info.max_satiation_level,
        );
        inventory_label
            .set_text(npc_resource_header_text(info.food_pouch, info.carried_resource).as_str());
        inventory_container.show();
        self.sync_inventory_rows(info.carried_resource);
    }

    fn clear_and_hide(&mut self) {
        self.selected_npc_entity_id = None;

        let mut name_label = self.name_label.clone();
        let mut age_label = self.age_label.clone();
        let mut birth_day_label = self.birth_day_label.clone();
        let mut pos_label = self.pos_label.clone();
        let mut hunger_label = self.hunger_label.clone();
        let mut satiation_container = self.satiation_container.clone();
        let mut satiation_progress_bar = self.satiation_progress_bar.clone();
        let mut inventory_container = self.inventory_container.clone();
        let mut inventory_label = self.inventory_label.clone();

        name_label.set_text("Name: None");
        age_label.set_text("");
        birth_day_label.set_text("");
        pos_label.set_text("Cell: None");
        hunger_label.set_text("");
        satiation_progress_bar.set_value(0.0);
        satiation_progress_bar.set_tooltip_text("");
        satiation_container.hide();
        inventory_label.set_text("Food Pouch:\nCarried Resource: Empty");
        inventory_container.hide();
        self.sync_inventory_rows(game_engine::npcs::CarriedResource::empty());
        self.set_details_button_enabled(false);
        self.base_mut().hide();
    }

    fn set_details_button_enabled(&mut self, enabled: bool) {
        let mut details_button = self.details_button.clone();
        let selected = if enabled {
            self.selected_npc_entity_id
        } else {
            None
        };
        details_button.set_disabled(!details_button_enabled(selected));
    }

    fn sync_inventory_rows(&mut self, inventory: game_engine::npcs::CarriedResource) {
        let Some(scene) = self.resource_quantity_scene.as_ref() else {
            return;
        };
        let contents = inventory.contents();
        let kinds = ResourceKind::ALL
            .into_iter()
            .filter(|kind| contents.get(*kind) > 0)
            .collect::<Vec<_>>();
        if self
            .inventory_rows
            .iter()
            .map(|row| row.kind)
            .collect::<Vec<_>>()
            != kinds
        {
            for mut row in self.inventory_rows.drain(..) {
                row.node.queue_free();
            }
            let mut container = self.inventory_rows_container.clone();
            for kind in &kinds {
                let Some(node) = scene.instantiate() else {
                    godot_error!("NpcInfoPanel: failed to instantiate inventory row");
                    return;
                };
                let Ok(mut node) = node.try_cast::<ResourceQuantity>() else {
                    godot_error!("NpcInfoPanel: inventory row has unexpected root type");
                    return;
                };
                node.bind_mut().set_resource_kind(*kind);
                container.add_child(&node);
                self.inventory_rows
                    .push(InventoryRowControl { kind: *kind, node });
            }
        }
        for row in &mut self.inventory_rows {
            row.node.bind_mut().set_amount(contents.get(row.kind));
        }
    }
}
