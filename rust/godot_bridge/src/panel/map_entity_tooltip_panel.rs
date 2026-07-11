use crate::world::game_world::{decode_entity_id, GameWorld, MapEntityKind};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::buildings::{
    Building, BuildingBlueprint, BuildingFootprint, ConstructionProgress,
};
use game_engine::components::{Tile, TilePosition};
use game_engine::housing::housing_snapshot;
use game_engine::npcs::{
    BirthDate, CarriedResource, FoodPouch, Npc, NpcName, NpcPosition, WorldDateTime,
};
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::{ResourceAmounts, ResourceKind};
use godot::classes::{control, IPanelContainer, PanelContainer, RichTextLabel};
use godot::obj::OnEditor;
use godot::prelude::*;

const TOOLTIP_CURSOR_OFFSET: Vector2 = Vector2::new(16.0, 16.0);

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct MapEntityTooltipPanel {
    #[export]
    text_label: OnEditor<Gd<RichTextLabel>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    hovered_target: Option<(MapEntityKind, i64)>,
    cached_text: Option<String>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for MapEntityTooltipPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            text_label: OnEditor::default(),
            game_world: OnEditor::default(),
            hovered_target: None,
            cached_text: None,
            base,
        }
    }

    fn ready(&mut self) {
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
        self.cached_text = None;
        self.refresh_tooltip();
        if self.hovered_target.is_none() {
            return;
        }
        self.base_mut().show();
        self.position_near_mouse();
    }

    fn hide_tooltip(&mut self) {
        self.hovered_target = None;
        self.cached_text = None;
        self.base_mut().hide();
    }

    fn refresh_tooltip(&mut self) {
        let Some((kind, entity_id)) = self.hovered_target else {
            return;
        };
        let text = {
            let game_world = self.game_world.bind();
            map_entity_tooltip_text(&game_world, kind, entity_id)
        };
        let Some(text) = text else {
            self.hide_tooltip();
            return;
        };
        if self.cached_text.as_ref() == Some(&text) {
            return;
        }

        self.text_label.clone().parse_bbcode(text.as_str());
        self.cached_text = Some(text);
        self.base_mut().reset_size();
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

fn map_entity_tooltip_text(
    game_world: &GameWorld,
    kind: MapEntityKind,
    entity_id: i64,
) -> Option<String> {
    let entity = decode_entity_id(entity_id)?;
    game_world.with_rendered_surface_world(|world| match kind {
        MapEntityKind::Building => building_tooltip_text(world, entity),
        MapEntityKind::Npc => npc_tooltip_text(world, entity),
        MapEntityKind::ResourceNode => resource_node_tooltip_text(world, entity),
    })
}

fn building_tooltip_text(world: &World, entity: Entity) -> Option<String> {
    if let Some(blueprint) = world.get::<BuildingBlueprint>(entity) {
        let progress = world.get::<ConstructionProgress>(entity)?;

        return Some(format_building_blueprint_tooltip(
            blueprint.kind.label(),
            blueprint.footprint,
            progress.deposited(),
            blueprint.kind.definition().construction_cost(),
        ));
    }

    let building = world.get::<Building>(entity)?;
    let occupancy = housing_snapshot(world)
        .house(entity)
        .map(|house| (house.occupied(), house.capacity()));
    Some(format_finished_building_tooltip(
        building.kind.label(),
        building.footprint,
        occupancy,
    ))
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

fn format_building_blueprint_tooltip(
    label: &str,
    footprint: BuildingFootprint,
    progress: ResourceAmounts,
    cost: ResourceAmounts,
) -> String {
    let origin = footprint.origin();
    format!(
        "[b]{} Blueprint[/b]\nCell: ({}, {})\nFootprint: {}x{}\nConstruction: {}",
        label,
        origin.x(),
        origin.y(),
        footprint.width(),
        footprint.height(),
        format_deposited_over_required(progress, cost)
    )
}

fn format_finished_building_tooltip(
    label: &str,
    footprint: BuildingFootprint,
    occupancy: Option<(usize, usize)>,
) -> String {
    let origin = footprint.origin();
    let mut text = format!(
        "[b]{}[/b]\nBuilding\nCell: ({}, {})\nFootprint: {}x{}",
        label,
        origin.x(),
        origin.y(),
        footprint.width(),
        footprint.height()
    );
    if let Some((occupied, capacity)) = occupancy {
        text.push_str(format!("\nOccupancy: {occupied}/{capacity}").as_str());
    }
    text
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

fn format_deposited_over_required(progress: ResourceAmounts, cost: ResourceAmounts) -> String {
    let parts = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let required = cost.get(kind);
            (required > 0).then(|| format!("{}: {}/{}", kind.label(), progress.get(kind), required))
        })
        .collect::<Vec<_>>();

    format_parts_or_none(parts)
}

fn format_parts_or_none(parts: Vec<String>) -> String {
    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::grid::CellCoord;

    #[test]
    fn building_tooltip_formats_footprint_and_progress() {
        let text = format_building_blueprint_tooltip(
            "Warehouse",
            BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
            ResourceAmounts::new(5, 0, 0, 0),
            ResourceAmounts::new(40, 20, 0, 0),
        );

        assert_eq!(
            text,
            "[b]Warehouse Blueprint[/b]\nCell: (4, 7)\nFootprint: 2x2\nConstruction: Wood: 5/40, Stone: 0/20"
        );
    }

    #[test]
    fn finished_building_tooltip_omits_construction_progress() {
        let text = format_finished_building_tooltip(
            "Warehouse",
            BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
            None,
        );

        assert_eq!(
            text,
            "[b]Warehouse[/b]\nBuilding\nCell: (4, 7)\nFootprint: 2x2"
        );
    }

    #[test]
    fn finished_house_tooltip_shows_occupancy_without_resident_details() {
        let text = format_finished_building_tooltip(
            "Medium House",
            BuildingFootprint::new(CellCoord::new(4, 7), 2, 2),
            Some((3, 4)),
        );

        assert_eq!(
            text,
            "[b]Medium House[/b]\nBuilding\nCell: (4, 7)\nFootprint: 2x2\nOccupancy: 3/4"
        );
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
