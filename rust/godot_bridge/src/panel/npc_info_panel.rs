use super::resource_quantity::ResourceQuantity;
use crate::world::game_world::{decode_entity_id, GameWorld};
use game_engine::grid::CellCoord;
use game_engine::npcs::{
    BirthDate, HungerState, Npc, NpcHunger, NpcInventory, NpcName, NpcPosition, WorldDateTime,
};
use game_engine::resources::{ResourceAmounts, ResourceKind};
use game_engine::time::SECONDS_PER_DAY;
use godot::classes::{IPanelContainer, Label, PanelContainer, VBoxContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

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

    selected_npc_entity_id: Option<i64>,
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
            inventory_container: OnEditor::default(),
            wood_inventory_quantity: OnEditor::default(),
            stone_inventory_quantity: OnEditor::default(),
            food_inventory_quantity: OnEditor::default(),
            gold_inventory_quantity: OnEditor::default(),
            game_world: OnEditor::default(),
            selected_npc_entity_id: None,
            base,
        }
    }

    fn ready(&mut self) {
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

        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        self.refresh_selected_npc();
    }
}

impl NpcInfoPanel {
    fn select_npc(&mut self, npc_entity_id: i64) {
        self.selected_npc_entity_id = Some(npc_entity_id);
        self.refresh_selected_npc();
    }

    fn deselect_npc(&mut self) {
        self.selected_npc_entity_id = None;
        self.clear_npc_labels();
    }

    fn refresh_selected_npc(&mut self) {
        let Some(npc_entity_id) = self.selected_npc_entity_id else {
            return;
        };
        let info = {
            let game_world = self.game_world.bind();
            npc_info(&game_world, npc_entity_id)
        };

        let Some(info) = info else {
            self.selected_npc_entity_id = None;
            self.clear_npc_labels();
            return;
        };

        self.update_npc_labels(info);
    }

    fn update_npc_labels(&mut self, info: NpcInfo) {
        let mut name_label = self.name_label.clone();
        let mut age_label = self.age_label.clone();
        let mut birth_day_label = self.birth_day_label.clone();
        let mut pos_label = self.pos_label.clone();
        let mut hunger_label = self.hunger_label.clone();
        let mut inventory_container = self.inventory_container.clone();
        let mut wood_inventory_quantity = self.wood_inventory_quantity.clone();
        let mut stone_inventory_quantity = self.stone_inventory_quantity.clone();
        let mut food_inventory_quantity = self.food_inventory_quantity.clone();
        let mut gold_inventory_quantity = self.gold_inventory_quantity.clone();

        let name_text = format!("Name: {}", info.name);
        let age_text = format!("Age: {}", info.age_years);
        let birth_day_text = format!("Birth Day: {}", info.birth_day);
        let position_text = format!("Cell: ({}, {})", info.coord.x(), info.coord.y());
        let hunger_text = format!("Hunger: {}", info.hunger_state.label());
        name_label.set_text(name_text.as_str());
        age_label.set_text(age_text.as_str());
        birth_day_label.set_text(birth_day_text.as_str());
        pos_label.set_text(position_text.as_str());
        hunger_label.set_text(hunger_text.as_str());
        update_inventory(
            &mut inventory_container,
            &mut wood_inventory_quantity,
            &mut stone_inventory_quantity,
            &mut food_inventory_quantity,
            &mut gold_inventory_quantity,
            info.inventory,
        );
    }

    fn clear_npc_labels(&mut self) {
        let mut name_label = self.name_label.clone();
        let mut age_label = self.age_label.clone();
        let mut birth_day_label = self.birth_day_label.clone();
        let mut pos_label = self.pos_label.clone();
        let mut hunger_label = self.hunger_label.clone();
        let mut inventory_container = self.inventory_container.clone();

        name_label.set_text("Name: None");
        age_label.set_text("");
        birth_day_label.set_text("");
        pos_label.set_text("Cell: None");
        hunger_label.set_text("");
        inventory_container.hide();
    }
}

struct NpcInfo {
    coord: CellCoord,
    name: String,
    birth_day: u64,
    age_years: u32,
    hunger_state: HungerState,
    inventory: ResourceAmounts,
}

fn npc_info(game_world: &GameWorld, npc_entity_id: i64) -> Option<NpcInfo> {
    let entity = decode_entity_id(npc_entity_id)?;
    game_world.with_rendered_surface_world(|world| {
        world.get::<Npc>(entity)?;
        let position = world.get::<NpcPosition>(entity)?;
        let name = world.get::<NpcName>(entity)?;
        let birth_date = world.get::<BirthDate>(entity)?;
        let hunger = world.get::<NpcHunger>(entity)?;
        let inventory = world.get::<NpcInventory>(entity)?;
        let world_date_time = *world.resource::<WorldDateTime>();

        Some(NpcInfo {
            coord: position.coord,
            name: name.as_str().to_string(),
            birth_day: birth_date.elapsed_since_world_epoch().as_secs() / SECONDS_PER_DAY,
            age_years: world_date_time.age_years_since(*birth_date),
            hunger_state: hunger.state(),
            inventory: inventory.contents(),
        })
    })
}

fn update_inventory(
    inventory_container: &mut Gd<VBoxContainer>,
    wood_quantity: &mut Gd<ResourceQuantity>,
    stone_quantity: &mut Gd<ResourceQuantity>,
    food_quantity: &mut Gd<ResourceQuantity>,
    gold_quantity: &mut Gd<ResourceQuantity>,
    inventory: ResourceAmounts,
) {
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
}
