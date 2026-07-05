use super::resource_quantity::ResourceQuantity;
use crate::world::game_world::{decode_entity_id, GameWorld};
use game_engine::components::{Tile, TilePosition};
use game_engine::grid::CellCoord;
use game_engine::resource_nodes::ResourceNode;
use godot::classes::{IPanelContainer, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct TileInfoPanel {
    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    resource_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for TileInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            pos_label: OnEditor::default(),
            resource_quantity: OnEditor::default(),
            game_world: OnEditor::default(),
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        let pos_label = self.pos_label.clone();
        let resource_quantity = self.resource_quantity.clone();

        let selected_game_world = game_world.clone();
        let mut selected_pos_label = pos_label.clone();
        let mut selected_resource_quantity = resource_quantity.clone();
        game_world
            .signals()
            .tile_selected()
            .connect(move |tile_entity_id| {
                let game_world = selected_game_world.bind();
                let Some(info) = tile_info(&game_world, tile_entity_id) else {
                    clear_tile_info(&mut selected_pos_label, &mut selected_resource_quantity);
                    return;
                };

                let position_text = format!("Cell: ({}, {})", info.coord.x(), info.coord.y());
                selected_pos_label.set_text(position_text.as_str());
                update_resource_quantity(&mut selected_resource_quantity, info.resource);
            });

        let mut deselected_pos_label = pos_label;
        let mut deselected_resource_quantity = resource_quantity;
        game_world.signals().tile_deselected().connect(move || {
            clear_tile_info(&mut deselected_pos_label, &mut deselected_resource_quantity);
        });
    }
}

struct TileInfo {
    coord: CellCoord,
    resource: Option<ResourceNode>,
}

fn tile_info(game_world: &GameWorld, tile_entity_id: i64) -> Option<TileInfo> {
    let entity = decode_entity_id(tile_entity_id)?;
    game_world.with_rendered_surface_world(|world| {
        world.get::<Tile>(entity)?;
        let position = world.get::<TilePosition>(entity)?;
        let resource = world.get::<ResourceNode>(entity).copied();

        Some(TileInfo {
            coord: position.coord,
            resource,
        })
    })
}

fn update_resource_quantity(
    resource_quantity: &mut Gd<ResourceQuantity>,
    resource: Option<ResourceNode>,
) {
    let mut resource_quantity = resource_quantity.bind_mut();
    if let Some(resource) = resource {
        resource_quantity.set_resource_kind(resource.kind);
        resource_quantity.set_amount(resource.quantity);
        resource_quantity.show_quantity();
    } else {
        resource_quantity.hide_quantity();
    }
}

fn clear_tile_info(pos_label: &mut Gd<Label>, resource_quantity: &mut Gd<ResourceQuantity>) {
    pos_label.set_text("Cell: None");
    resource_quantity.bind_mut().hide_quantity();
}
