use super::resource_quantity::ResourceQuantity;
use crate::assets::load_packed_scene;
use crate::world::game_world::{decode_entity_id, GameWorld};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::grid::CellCoord;
use game_engine::npcs::{
    BirthDate, CarriedResource, FoodPouch, HungerState, Npc, NpcHunger, NpcName, NpcPosition,
    NpcSkills, SkillKind, SkillRank, WorldDateTime, MAX_SKILL_VALUE,
};
use game_engine::resources::ResourceKind;
use game_engine::time::SECONDS_PER_DAY;
use godot::classes::{
    control, Button, GridContainer, IPanelContainer, Label, PackedScene, PanelContainer,
    ProgressBar, VBoxContainer,
};
use godot::obj::{NewAlloc, OnEditor};
use godot::prelude::*;

const RESOURCE_QUANTITY_SCENE_PATH: &str = "res://panel/resource_quantity.tscn";

struct InventoryRowControl {
    kind: ResourceKind,
    node: Gd<ResourceQuantity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NpcDetails {
    pub(crate) coord: CellCoord,
    pub(crate) name: String,
    pub(crate) birth_day: u64,
    pub(crate) age_years: u32,
    pub(crate) hunger_state: HungerState,
    pub(crate) satiation_level: u32,
    pub(crate) max_satiation_level: u32,
    pub(crate) food_pouch: FoodPouch,
    pub(crate) carried_resource: CarriedResource,
    pub(crate) skills: Vec<NpcSkillDetails>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NpcSkillDetails {
    pub(crate) kind: SkillKind,
    pub(crate) value: u32,
    pub(crate) percent: u32,
    pub(crate) rank: SkillRank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DetailsView {
    Empty,
    Details(NpcDetails),
}

pub(crate) fn npc_details(game_world: &GameWorld, npc_entity_id: i64) -> Option<NpcDetails> {
    let entity = decode_entity_id(npc_entity_id)?;
    game_world.with_rendered_surface_world(|world| npc_details_from_world(world, entity))
}

pub(crate) fn npc_details_from_world(world: &World, entity: Entity) -> Option<NpcDetails> {
    world.get::<Npc>(entity)?;
    let position = world.get::<NpcPosition>(entity)?;
    let name = world.get::<NpcName>(entity)?;
    let birth_date = world.get::<BirthDate>(entity)?;
    let hunger = world.get::<NpcHunger>(entity)?;
    let food_pouch = world.get::<FoodPouch>(entity)?;
    let carried_resource = world.get::<CarriedResource>(entity)?;
    let skills = world.get::<NpcSkills>(entity).copied().unwrap_or_default();
    let world_date_time = *world.resource::<WorldDateTime>();

    Some(NpcDetails {
        coord: position.coord,
        name: name.as_str().to_string(),
        birth_day: birth_date.elapsed_since_world_epoch().as_secs() / SECONDS_PER_DAY,
        age_years: world_date_time.age_years_since(*birth_date),
        hunger_state: hunger.state(),
        satiation_level: hunger.satiation_level(),
        max_satiation_level: NpcHunger::MAX_SATIATION_LEVEL,
        food_pouch: *food_pouch,
        carried_resource: *carried_resource,
        skills: skill_details(skills),
    })
}

pub(crate) fn details_button_enabled(selected_npc_entity_id: Option<i64>) -> bool {
    selected_npc_entity_id.is_some()
}

pub(crate) fn configure_satiation_progress_bar(
    satiation_progress_bar: &mut Gd<ProgressBar>,
    max_satiation_level: u32,
) {
    satiation_progress_bar.set_min(f64::from(NpcHunger::MIN_SATIATION_LEVEL));
    satiation_progress_bar.set_max(f64::from(max_satiation_level));
    satiation_progress_bar.set_show_percentage(false);
}

pub(crate) fn update_satiation(
    hunger_label: &mut Gd<Label>,
    satiation_container: &mut Gd<VBoxContainer>,
    satiation_progress_bar: &mut Gd<ProgressBar>,
    hunger_state: HungerState,
    satiation_level: u32,
    max_satiation_level: u32,
) {
    let text = hunger_text(hunger_state, satiation_level, max_satiation_level);
    hunger_label.set_text(text.as_str());

    configure_satiation_progress_bar(satiation_progress_bar, max_satiation_level);
    satiation_progress_bar.set_value(satiation_progress_value(
        satiation_level,
        max_satiation_level,
    ));
    satiation_progress_bar.set_tooltip_text(text.as_str());
    satiation_container.show();
}

pub(crate) fn hunger_text(
    hunger_state: HungerState,
    satiation_level: u32,
    max_satiation_level: u32,
) -> String {
    format!(
        "Hunger: {} ({}/{})",
        hunger_state.label(),
        satiation_level,
        max_satiation_level
    )
}

pub(crate) fn satiation_progress_value(satiation_level: u32, max_satiation_level: u32) -> f64 {
    f64::from(satiation_level.min(max_satiation_level))
}

pub(crate) fn npc_resource_header_text(
    food_pouch: FoodPouch,
    carried_resource: CarriedResource,
) -> String {
    let cargo = carried_resource.stack().map_or_else(
        || "Empty".to_string(),
        |stack| format!("{}: {}/5", stack.kind().label(), stack.amount()),
    );
    format!(
        "Food Pouch: {}/{}\nCarried Resource: {cargo}",
        food_pouch.amount(),
        food_pouch.capacity()
    )
}

fn nonzero_resource_kinds(contents: game_engine::resources::ResourceAmounts) -> Vec<ResourceKind> {
    ResourceKind::ALL
        .into_iter()
        .filter(|kind| contents.get(*kind) > 0)
        .collect()
}

pub(crate) fn skill_percent_text(percent: u32) -> String {
    format!("{}%", percent.min(100))
}

pub(crate) fn skill_raw_value_tooltip_text(value: u32) -> String {
    format!("{}/{}", value.min(MAX_SKILL_VALUE), MAX_SKILL_VALUE)
}

fn skill_details(skills: NpcSkills) -> Vec<NpcSkillDetails> {
    SkillKind::ALL
        .into_iter()
        .map(|kind| NpcSkillDetails {
            kind,
            value: skills.value(kind),
            percent: skills.percent(kind),
            rank: skills.rank(kind),
        })
        .collect()
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct NpcDetailsPanel {
    #[export]
    close_button: OnEditor<Gd<Button>>,

    #[export]
    open_button: OnEditor<Gd<Button>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    #[export]
    details_container: OnEditor<Gd<VBoxContainer>>,

    #[export]
    empty_state_label: OnEditor<Gd<Label>>,

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
    skills_grid: OnEditor<Gd<GridContainer>>,

    selected_npc_entity_id: Option<i64>,
    cached_view: Option<DetailsView>,
    resource_quantity_scene: Option<Gd<PackedScene>>,
    inventory_rows: Vec<InventoryRowControl>,
    skill_labels: Vec<Gd<Label>>,
    skill_progress_bars: Vec<Gd<ProgressBar>>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for NpcDetailsPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            close_button: OnEditor::default(),
            open_button: OnEditor::default(),
            game_world: OnEditor::default(),
            details_container: OnEditor::default(),
            empty_state_label: OnEditor::default(),
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
            skills_grid: OnEditor::default(),
            selected_npc_entity_id: None,
            cached_view: None,
            resource_quantity_scene: None,
            inventory_rows: Vec::new(),
            skill_labels: Vec::new(),
            skill_progress_bars: Vec::new(),
            base,
        }
    }

    fn ready(&mut self) {
        self.resource_quantity_scene =
            load_packed_scene(RESOURCE_QUANTITY_SCENE_PATH, "NpcDetailsPanel");

        let close_button = self.close_button.clone();
        close_button
            .signals()
            .pressed()
            .connect_other(self, Self::hide_panel);

        let open_button = self.open_button.clone();
        open_button
            .signals()
            .pressed()
            .connect_other(self, Self::show_panel);

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

        self.base_mut().hide();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh();
        }
    }
}

impl NpcDetailsPanel {
    fn hide_panel(&mut self) {
        self.base_mut().hide();
    }

    fn show_panel(&mut self) {
        self.cached_view = None;
        self.base_mut().show();
        self.refresh();
    }

    fn select_npc(&mut self, npc_entity_id: i64) {
        self.selected_npc_entity_id = Some(npc_entity_id);
        self.cached_view = None;
        if self.base().is_visible() {
            self.refresh();
        }
    }

    fn deselect_npc(&mut self) {
        self.selected_npc_entity_id = None;
        self.cached_view = None;
        if self.base().is_visible() {
            self.refresh();
        }
    }

    fn refresh(&mut self) {
        let view = match self.selected_npc_entity_id {
            Some(npc_entity_id) => {
                let details = {
                    let game_world = self.game_world.bind();
                    npc_details(&game_world, npc_entity_id)
                };
                match details {
                    Some(details) => DetailsView::Details(details),
                    None => {
                        self.selected_npc_entity_id = None;
                        DetailsView::Empty
                    }
                }
            }
            None => DetailsView::Empty,
        };

        if self.cached_view.as_ref() == Some(&view) {
            return;
        }

        self.render(view.clone());
        self.cached_view = Some(view);
    }

    fn render(&mut self, view: DetailsView) {
        match view {
            DetailsView::Empty => self.render_empty(),
            DetailsView::Details(details) => self.render_details(details),
        }
    }

    fn render_empty(&mut self) {
        let mut empty_state_label = self.empty_state_label.clone();
        let mut details_container = self.details_container.clone();
        empty_state_label.show();
        details_container.hide();
        self.sync_inventory_rows(CarriedResource::empty());
        self.clear_skill_rows();
    }

    fn render_details(&mut self, details: NpcDetails) {
        let mut empty_state_label = self.empty_state_label.clone();
        let mut details_container = self.details_container.clone();
        empty_state_label.hide();
        details_container.show();

        let mut name_label = self.name_label.clone();
        let mut age_label = self.age_label.clone();
        let mut birth_day_label = self.birth_day_label.clone();
        let mut pos_label = self.pos_label.clone();
        let mut hunger_label = self.hunger_label.clone();
        let mut satiation_container = self.satiation_container.clone();
        let mut satiation_progress_bar = self.satiation_progress_bar.clone();
        let mut inventory_container = self.inventory_container.clone();
        let mut inventory_label = self.inventory_label.clone();

        name_label.set_text(format!("Name: {}", details.name).as_str());
        age_label.set_text(format!("Age: {}", details.age_years).as_str());
        birth_day_label.set_text(format!("Birth Day: {}", details.birth_day).as_str());
        pos_label
            .set_text(format!("Cell: ({}, {})", details.coord.x(), details.coord.y()).as_str());
        update_satiation(
            &mut hunger_label,
            &mut satiation_container,
            &mut satiation_progress_bar,
            details.hunger_state,
            details.satiation_level,
            details.max_satiation_level,
        );
        inventory_label.set_text(
            npc_resource_header_text(details.food_pouch, details.carried_resource).as_str(),
        );
        inventory_container.show();
        self.sync_inventory_rows(details.carried_resource);
        self.rebuild_skill_rows(&details.skills);
    }

    fn sync_inventory_rows(&mut self, carried_resource: CarriedResource) {
        let Some(scene) = self.resource_quantity_scene.as_ref() else {
            return;
        };
        let contents = carried_resource.contents();
        let kinds = nonzero_resource_kinds(contents);
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
                    godot_error!("NpcDetailsPanel: failed to instantiate inventory row");
                    return;
                };
                let Ok(mut node) = node.try_cast::<ResourceQuantity>() else {
                    godot_error!("NpcDetailsPanel: inventory row has unexpected root type");
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

    fn clear_skill_rows(&mut self) {
        for mut label in self.skill_labels.drain(..) {
            label.queue_free();
        }
        for mut progress_bar in self.skill_progress_bars.drain(..) {
            progress_bar.queue_free();
        }
    }

    fn rebuild_skill_rows(&mut self, skills: &[NpcSkillDetails]) {
        self.clear_skill_rows();
        let mut skills_grid = self.skills_grid.clone();

        self.add_skill_label(&mut skills_grid, "Skill", "");
        self.add_skill_label(&mut skills_grid, "Percent", "");
        self.add_skill_label(&mut skills_grid, "Progress", "");
        self.add_skill_label(&mut skills_grid, "Rank", "");

        for skill in skills {
            let percent = skill_percent_text(skill.percent);
            let tooltip = skill_raw_value_tooltip_text(skill.value);
            self.add_skill_label(&mut skills_grid, skill.kind.label(), tooltip.as_str());
            self.add_skill_label(&mut skills_grid, percent.as_str(), tooltip.as_str());
            self.add_skill_progress_bar(&mut skills_grid, skill.percent, tooltip.as_str());
            self.add_skill_label(&mut skills_grid, skill.rank.label(), tooltip.as_str());
        }
    }

    fn add_skill_label(&mut self, skills_grid: &mut Gd<GridContainer>, text: &str, tooltip: &str) {
        let mut label = Label::new_alloc();
        label.set_text(text);
        label.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
        if !tooltip.is_empty() {
            label.set_tooltip_text(tooltip);
        }
        skills_grid.add_child(&label);
        self.skill_labels.push(label);
    }

    fn add_skill_progress_bar(
        &mut self,
        skills_grid: &mut Gd<GridContainer>,
        percent: u32,
        tooltip: &str,
    ) {
        let mut progress_bar = ProgressBar::new_alloc();
        progress_bar.set_min(0.0);
        progress_bar.set_max(100.0);
        progress_bar.set_value(f64::from(percent.min(100)));
        progress_bar.set_show_percentage(false);
        progress_bar.set_custom_minimum_size(Vector2::new(120.0, 12.0));
        progress_bar.set_h_size_flags(control::SizeFlags::EXPAND_FILL);
        progress_bar.set_tooltip_text(tooltip);
        skills_grid.add_child(&progress_bar);
        self.skill_progress_bars.push(progress_bar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::resources::ResourceAmounts;
    use std::time::Duration;

    #[test]
    fn npc_details_include_existing_fields_and_skills() {
        let mut world = World::new();
        world.insert_resource(WorldDateTime::from_day(400));
        let entity = world
            .spawn((
                Npc,
                NpcPosition::new(CellCoord::new(2, 3)),
                NpcName::new("Iris"),
                BirthDate::new(Duration::from_secs(35 * SECONDS_PER_DAY)),
                NpcHunger::new(12),
                FoodPouch::new(20),
                CarriedResource::of(ResourceKind::Stone, 2),
                NpcSkills::new([0, 0, 123, 2500, 5000, 10_000, 0, 0, 0]),
            ))
            .id();

        let details = npc_details_from_world(&world, entity).expect("NPC details should exist");

        assert_eq!(details.name, "Iris");
        assert_eq!(details.coord, CellCoord::new(2, 3));
        assert_eq!(details.birth_day, 35);
        assert_eq!(details.age_years, 1);
        assert_eq!(details.satiation_level, 12);
        assert_eq!(
            details.carried_resource.contents(),
            ResourceAmounts::of(ResourceKind::Stone, 2)
        );
        assert_eq!(details.skills.len(), SkillKind::ALL.len());
        assert_eq!(details.skills[0].kind, SkillKind::Builder);
        assert_eq!(details.skills[2].kind, SkillKind::Lumberjack);
        assert_eq!(details.skills[2].value, 123);
        assert_eq!(details.skills[2].percent, 1);
        assert_eq!(details.skills[3].rank, SkillRank::Journeyman);
        assert_eq!(details.skills[5].rank, SkillRank::GrandMaster);
    }

    #[test]
    fn missing_npc_skills_are_reported_as_zeroes() {
        let mut world = World::new();
        world.insert_resource(WorldDateTime::from_day(0));
        let entity = world
            .spawn((
                Npc,
                NpcPosition::new(CellCoord::new(0, 0)),
                NpcName::new("No Skills"),
                BirthDate::new(Duration::ZERO),
                NpcHunger::fed(),
                FoodPouch::empty(),
                CarriedResource::empty(),
            ))
            .id();

        let details = npc_details_from_world(&world, entity).expect("NPC details should exist");

        assert_eq!(details.skills.len(), SkillKind::ALL.len());
        for skill in details.skills {
            assert_eq!(skill.value, 0);
            assert_eq!(skill.percent, 0);
            assert_eq!(skill.rank, SkillRank::Untrained);
        }
    }

    #[test]
    fn formatting_helpers_match_panel_text() {
        assert_eq!(
            hunger_text(HungerState::Hungry, 12, 48),
            "Hunger: Hungry (12/48)"
        );
        assert_eq!(satiation_progress_value(12, 48), 12.0);
        assert_eq!(satiation_progress_value(80, 48), 48.0);
        assert_eq!(
            npc_resource_header_text(
                FoodPouch::new(20),
                CarriedResource::of(ResourceKind::Wood, 3)
            ),
            "Food Pouch: 20/100\nCarried Resource: Wood: 3/5"
        );
        assert_eq!(skill_percent_text(42), "42%");
        assert_eq!(skill_percent_text(142), "100%");
        assert_eq!(skill_raw_value_tooltip_text(12_000), "10000/10000");
    }

    #[test]
    fn inventory_rows_include_only_nonzero_resources_in_stable_order() {
        let contents = ResourceAmounts::zero()
            .with(ResourceKind::Food, 3)
            .with(ResourceKind::WildBerries, 2)
            .with(ResourceKind::StoneBlocks, 1);

        assert_eq!(
            nonzero_resource_kinds(contents),
            vec![
                ResourceKind::Food,
                ResourceKind::WildBerries,
                ResourceKind::StoneBlocks,
            ]
        );
    }

    #[test]
    fn details_button_state_follows_selection_presence() {
        assert!(!details_button_enabled(None));
        assert!(details_button_enabled(Some(42)));
    }
}
