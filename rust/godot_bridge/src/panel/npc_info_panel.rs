use crate::world::game_world::{decode_entity_id, GameWorld};
use game_engine::grid::CellCoord;
use game_engine::npcs::{BirthDate, Npc, NpcName, NpcPosition, WorldDateTime, SECONDS_PER_DAY};
use godot::classes::{IPanelContainer, Label, PanelContainer};
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
    game_world: OnEditor<Gd<GameWorld>>,

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
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        let name_label = self.name_label.clone();
        let age_label = self.age_label.clone();
        let birth_day_label = self.birth_day_label.clone();
        let pos_label = self.pos_label.clone();

        let selected_game_world = game_world.clone();
        let mut selected_name_label = name_label.clone();
        let mut selected_age_label = age_label.clone();
        let mut selected_birth_day_label = birth_day_label.clone();
        let mut selected_pos_label = pos_label.clone();
        game_world
            .signals()
            .npc_selected()
            .connect(move |npc_entity_id| {
                let game_world = selected_game_world.bind();
                let Some(info) = npc_info(&game_world, npc_entity_id) else {
                    clear_npc_labels(
                        &mut selected_name_label,
                        &mut selected_age_label,
                        &mut selected_birth_day_label,
                        &mut selected_pos_label,
                    );
                    return;
                };

                let name_text = format!("Name: {}", info.name);
                let age_text = format!("Age: {}", info.age_years);
                let birth_day_text = format!("Birth Day: {}", info.birth_day);
                let position_text = format!("Cell: ({}, {})", info.coord.x(), info.coord.y());
                selected_name_label.set_text(name_text.as_str());
                selected_age_label.set_text(age_text.as_str());
                selected_birth_day_label.set_text(birth_day_text.as_str());
                selected_pos_label.set_text(position_text.as_str());
            });

        let mut deselected_name_label = name_label;
        let mut deselected_age_label = age_label;
        let mut deselected_birth_day_label = birth_day_label;
        let mut deselected_pos_label = pos_label;
        game_world.signals().npc_deselected().connect(move || {
            clear_npc_labels(
                &mut deselected_name_label,
                &mut deselected_age_label,
                &mut deselected_birth_day_label,
                &mut deselected_pos_label,
            );
        });
    }
}

struct NpcInfo {
    coord: CellCoord,
    name: String,
    birth_day: u64,
    age_years: u32,
}

fn npc_info(game_world: &GameWorld, npc_entity_id: i64) -> Option<NpcInfo> {
    let entity = decode_entity_id(npc_entity_id)?;
    game_world
        .with_rendered_surface_world(|world| {
            world.get::<Npc>(entity)?;
            let position = world.get::<NpcPosition>(entity)?;
            let name = world.get::<NpcName>(entity)?;
            let birth_date = world.get::<BirthDate>(entity)?;
            let world_date_time = *world.resource::<WorldDateTime>();

            Some(NpcInfo {
                coord: position.coord,
                name: name.as_str().to_string(),
                birth_day: birth_date.elapsed_since_world_epoch().as_secs() / SECONDS_PER_DAY,
                age_years: world_date_time.age_years_since(*birth_date),
            })
        })
        .flatten()
}

fn clear_npc_labels(
    name_label: &mut Gd<Label>,
    age_label: &mut Gd<Label>,
    birth_day_label: &mut Gd<Label>,
    pos_label: &mut Gd<Label>,
) {
    name_label.set_text("Name: None");
    age_label.set_text("");
    birth_day_label.set_text("");
    pos_label.set_text("Cell: None");
}
