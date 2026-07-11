use super::resource_quantity::ResourceQuantity;
use crate::world::game_world::{decode_entity_id, GameWorld};
use bevy_ecs::prelude::Entity;
use bevy_ecs::world::World;
use game_engine::components::{Terrain, TerrainKind, Tile, TilePosition};
use game_engine::grid::CellCoord;
use game_engine::resource_nodes::ResourceNode;
use game_engine::resources::ResourceKind;
use game_engine::roads::{road_cell_view, RoadCellView};
use godot::classes::{IPanelContainer, Label, PanelContainer};
use godot::obj::OnEditor;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub(crate) struct TileInfoPanel {
    #[export]
    pos_label: OnEditor<Gd<Label>>,

    #[export]
    terrain_label: OnEditor<Gd<Label>>,

    #[export]
    resource_quantity: OnEditor<Gd<ResourceQuantity>>,

    #[export]
    road_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    selected_tile_entity_id: Option<i64>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for TileInfoPanel {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            pos_label: OnEditor::default(),
            terrain_label: OnEditor::default(),
            resource_quantity: OnEditor::default(),
            road_label: OnEditor::default(),
            game_world: OnEditor::default(),
            selected_tile_entity_id: None,
            base,
        }
    }

    fn ready(&mut self) {
        let game_world = self.game_world.clone();
        game_world
            .signals()
            .tile_selected()
            .connect_other(self, Self::select_tile);

        let game_world = self.game_world.clone();
        game_world
            .signals()
            .tile_deselected()
            .connect_other(self, Self::deselect_tile);

        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        self.refresh_selected_tile();
    }
}

impl TileInfoPanel {
    fn select_tile(&mut self, tile_entity_id: i64) {
        self.selected_tile_entity_id = Some(tile_entity_id);
        self.refresh_selected_tile();
    }

    fn deselect_tile(&mut self) {
        self.selected_tile_entity_id = None;
        self.clear_tile_info();
    }

    fn refresh_selected_tile(&mut self) {
        let Some(tile_entity_id) = self.selected_tile_entity_id else {
            return;
        };
        let info = {
            let game_world = self.game_world.bind();
            tile_info(&game_world, tile_entity_id)
        };

        let Some(info) = info else {
            self.selected_tile_entity_id = None;
            self.clear_tile_info();
            return;
        };

        self.update_tile_info(info);
    }

    fn update_tile_info(&mut self, info: TileInfo) {
        let mut pos_label = self.pos_label.clone();
        let mut terrain_label = self.terrain_label.clone();
        let mut resource_quantity = self.resource_quantity.clone();
        let mut road_label = self.road_label.clone();

        let position_text = format!("Cell: ({}, {})", info.coord.x(), info.coord.y());
        pos_label.set_text(position_text.as_str());
        terrain_label.set_text(terrain_text(info.terrain).as_str());
        update_resource_quantity(&mut resource_quantity, info.resource);
        road_label.set_text(road_text(info.road).as_str());
    }

    fn clear_tile_info(&mut self) {
        let mut pos_label = self.pos_label.clone();
        let mut terrain_label = self.terrain_label.clone();
        let mut resource_quantity = self.resource_quantity.clone();
        let mut road_label = self.road_label.clone();

        clear_tile_info(&mut pos_label, &mut terrain_label, &mut resource_quantity);
        road_label.set_text("Road: None");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileInfo {
    coord: CellCoord,
    terrain: TerrainKind,
    resource: Option<ResourceNode>,
    road: Option<RoadCellView>,
}

fn tile_info(game_world: &GameWorld, tile_entity_id: i64) -> Option<TileInfo> {
    let entity = decode_entity_id(tile_entity_id)?;
    game_world.with_rendered_surface_world(|world| tile_info_from_world(world, entity))
}

fn tile_info_from_world(world: &World, entity: Entity) -> Option<TileInfo> {
    world.get::<Tile>(entity)?;
    let position = world.get::<TilePosition>(entity)?;
    let terrain = world.get::<Terrain>(entity)?;
    let resource = world.get::<ResourceNode>(entity).copied();
    let road = road_cell_view(world, position.coord);

    Some(TileInfo {
        coord: position.coord,
        terrain: terrain.kind,
        resource,
        road,
    })
}

fn road_text(road: Option<RoadCellView>) -> String {
    let Some(road) = road else {
        return "Road: None".to_owned();
    };
    let completed = road.completed_tier.map_or("None".to_owned(), |tier| {
        let (numerator, denominator) = tier.movement_ratio();
        let multiplier = numerator as f32 / denominator as f32;
        format!("{} ({multiplier:.1}×)", tier.label())
    });
    let Some(target) = road.target_tier else {
        return format!("Road: {completed}");
    };
    let progress = road
        .construction
        .expect("road blueprint has construction progress");
    let cost = target.material_cost();
    let materials = ResourceKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let required = cost.get(kind);
            (required > 0).then(|| {
                format!(
                    "{} {}/{}",
                    kind.label(),
                    progress.deposited().get(kind),
                    required
                )
            })
        })
        .collect::<Vec<_>>()
        .join(", ");
    let materials = if materials.is_empty() {
        "None"
    } else {
        materials.as_str()
    };
    let operation = if road.completed_tier.is_some() {
        "Upgrade"
    } else {
        "Blueprint"
    };
    format!(
        "Road: {completed}\n{operation} to {}\nMaterials: {materials}\nLabor: {}/{}",
        target.label(),
        progress.labor_completed(),
        progress.labor_required()
    )
}

fn terrain_text(terrain: TerrainKind) -> String {
    format!("Terrain: {}", terrain.label())
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

fn clear_tile_info(
    pos_label: &mut Gd<Label>,
    terrain_label: &mut Gd<Label>,
    resource_quantity: &mut Gd<ResourceQuantity>,
) {
    pos_label.set_text("Cell: None");
    terrain_label.set_text("Terrain: None");
    resource_quantity.bind_mut().hide_quantity();
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::resources::ResourceKind;
    use game_engine::tile::TileBundle;

    #[test]
    fn terrain_text_shows_terrain_label() {
        assert_eq!(terrain_text(TerrainKind::Grass), "Terrain: Grass");
        assert_eq!(terrain_text(TerrainKind::Sand), "Terrain: Sand");
        assert_eq!(terrain_text(TerrainKind::Dirt), "Terrain: Dirt");
        assert_eq!(terrain_text(TerrainKind::Water), "Terrain: Water");
    }

    #[test]
    fn tile_info_reads_current_resource_quantity() {
        let mut world = World::new();
        let coord = CellCoord::new(2, 3);
        let tile = world
            .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Dirt))
            .id();
        world.entity_mut(tile).insert(ResourceNode {
            kind: ResourceKind::Wood,
            quantity: 12,
        });

        assert_eq!(
            tile_info_from_world(&world, tile).and_then(|info| info.resource),
            Some(ResourceNode {
                kind: ResourceKind::Wood,
                quantity: 12,
            })
        );

        world
            .get_mut::<ResourceNode>(tile)
            .expect("test tile should have a resource node")
            .quantity = 11;

        assert_eq!(
            tile_info_from_world(&world, tile).and_then(|info| info.resource),
            Some(ResourceNode {
                kind: ResourceKind::Wood,
                quantity: 11,
            })
        );
    }

    #[test]
    fn tile_info_hides_resource_after_node_is_removed() {
        let mut world = World::new();
        let coord = CellCoord::new(4, 5);
        let tile = world
            .spawn(TileBundle::new_with_terrain(coord, TerrainKind::Grass))
            .id();
        world.entity_mut(tile).insert(ResourceNode {
            kind: ResourceKind::Food,
            quantity: 1,
        });

        assert!(tile_info_from_world(&world, tile)
            .expect("tile info should exist")
            .resource
            .is_some());

        world.entity_mut(tile).remove::<ResourceNode>();

        assert_eq!(
            tile_info_from_world(&world, tile),
            Some(TileInfo {
                coord,
                terrain: TerrainKind::Grass,
                resource: None,
                road: None,
            })
        );
    }
}
