use game_engine::buildings::BuildingKind;
use game_engine::components::{NpcAppearance, TerrainKind};
use game_engine::resources::ResourceKind;
use game_engine::roads::RoadTier;
use godot::classes::{PackedScene, ResourceLoader, Texture2D};
use godot::obj::Singleton;
use godot::prelude::*;

const TERRAIN_GRASS_PATH: &str = "res://assets/visual/world/terrain/terrain_grass.png";
const TERRAIN_SAND_PATH: &str = "res://assets/visual/world/terrain/terrain_sand.png";
const TERRAIN_DIRT_PATH: &str = "res://assets/visual/world/terrain/terrain_dirt.png";
const TERRAIN_WATER_PATH: &str = "res://assets/visual/world/terrain/terrain_water.png";
const RESOURCE_WOOD_PATH: &str = "res://assets/visual/world/resources/resource_wood.png";
const RESOURCE_STONE_PATH: &str = "res://assets/visual/world/resources/resource_stone.png";
const RESOURCE_FOOD_PATH: &str = "res://assets/visual/world/resources/resource_food.png";
const RESOURCE_GOLD_PATH: &str = "res://assets/visual/world/resources/resource_gold.png";
const RESOURCE_CROPS_PATH: &str = "res://assets/visual/world/resources/resource_crops.png";
const RESOURCE_WILD_BERRIES_PATH: &str =
    "res://assets/visual/world/resources/resource_wild_berries.png";
const RESOURCE_PLANKS_PATH: &str = "res://assets/visual/world/resources/resource_planks.png";
const RESOURCE_STONE_BLOCKS_PATH: &str =
    "res://assets/visual/world/resources/resource_stone_blocks.png";
const NPC_COLONIST_SCENE_PATH: &str = "res://world/npc_colonist.tscn";
const NPC_ENGINEER_SCENE_PATH: &str = "res://world/npc_engineer.tscn";
const NPC_BOTANIST_SCENE_PATH: &str = "res://world/npc_botanist.tscn";
const NPC_MINER_SCENE_PATH: &str = "res://world/npc_miner.tscn";
const NPC_SCOUT_SCENE_PATH: &str = "res://world/npc_scout.tscn";
const BUILDING_DEPOT_PATH: &str = "res://assets/visual/world/buildings/building_depot.png";
const BUILDING_WAREHOUSE_PATH: &str = "res://assets/visual/world/buildings/building_warehouse.png";
const BUILDING_TOWNHALL_PATH: &str = "res://assets/visual/world/buildings/building_townhall.png";
const BUILDING_SAWMILL_PATH: &str = "res://assets/visual/world/buildings/building_sawmill.png";
const BUILDING_STONEWORKS_PATH: &str =
    "res://assets/visual/world/buildings/building_stoneworks.png";
const BUILDING_KITCHEN_PATH: &str = "res://assets/visual/world/buildings/building_kitchen.png";
const BUILDING_FARM_PATH: &str = "res://assets/visual/world/buildings/building_farm.png";
const BUILDING_FIELD_PATH: &str = "res://assets/visual/world/buildings/building_field.png";
const BUILDING_FORESTER_LODGE_PATH: &str =
    "res://assets/visual/world/buildings/building_forester_lodge.png";
const BUILDING_TREE_PLOT_PATH: &str = "res://assets/visual/world/buildings/building_tree_plot.png";
const BUILDING_SMALL_HOUSE_PATH: &str =
    "res://assets/visual/world/buildings/building_house_small.png";
const BUILDING_MEDIUM_HOUSE_PATH: &str =
    "res://assets/visual/world/buildings/building_house_medium.png";
const BUILDING_LARGE_HOUSE_PATH: &str =
    "res://assets/visual/world/buildings/building_house_large.png";
const ROAD_DIRT_PATH: &str = "res://assets/visual/world/roads/road_dirt_path_atlas.png";
const ROAD_COBBLESTONE_PATH: &str = "res://assets/visual/world/roads/road_cobblestone_atlas.png";
const ROAD_FLAGSTONE_PATH: &str = "res://assets/visual/world/roads/road_flagstone_atlas.png";

pub(crate) fn terrain_asset_path(kind: TerrainKind) -> &'static str {
    match kind {
        TerrainKind::Grass => TERRAIN_GRASS_PATH,
        TerrainKind::Sand => TERRAIN_SAND_PATH,
        TerrainKind::Dirt => TERRAIN_DIRT_PATH,
        TerrainKind::Water => TERRAIN_WATER_PATH,
    }
}

pub(crate) fn resource_asset_path(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => RESOURCE_WOOD_PATH,
        ResourceKind::Stone => RESOURCE_STONE_PATH,
        ResourceKind::Food => RESOURCE_FOOD_PATH,
        ResourceKind::Gold => RESOURCE_GOLD_PATH,
        ResourceKind::Crops => RESOURCE_CROPS_PATH,
        ResourceKind::WildBerries => RESOURCE_WILD_BERRIES_PATH,
        ResourceKind::Planks => RESOURCE_PLANKS_PATH,
        ResourceKind::StoneBlocks => RESOURCE_STONE_BLOCKS_PATH,
    }
}

pub(crate) fn npc_scene_path(appearance: NpcAppearance) -> &'static str {
    match appearance {
        NpcAppearance::Colonist => NPC_COLONIST_SCENE_PATH,
        NpcAppearance::Engineer => NPC_ENGINEER_SCENE_PATH,
        NpcAppearance::Botanist => NPC_BOTANIST_SCENE_PATH,
        NpcAppearance::Miner => NPC_MINER_SCENE_PATH,
        NpcAppearance::Scout => NPC_SCOUT_SCENE_PATH,
    }
}

pub(crate) const fn building_asset_path(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Depot => BUILDING_DEPOT_PATH,
        BuildingKind::Warehouse => BUILDING_WAREHOUSE_PATH,
        BuildingKind::TownHall => BUILDING_TOWNHALL_PATH,
        BuildingKind::Sawmill => BUILDING_SAWMILL_PATH,
        BuildingKind::Stoneworks => BUILDING_STONEWORKS_PATH,
        BuildingKind::Kitchen => BUILDING_KITCHEN_PATH,
        BuildingKind::Farm => BUILDING_FARM_PATH,
        BuildingKind::Field => BUILDING_FIELD_PATH,
        BuildingKind::ForesterLodge => BUILDING_FORESTER_LODGE_PATH,
        BuildingKind::TreePlot => BUILDING_TREE_PLOT_PATH,
        BuildingKind::SmallHouse => BUILDING_SMALL_HOUSE_PATH,
        BuildingKind::MediumHouse => BUILDING_MEDIUM_HOUSE_PATH,
        BuildingKind::LargeHouse => BUILDING_LARGE_HOUSE_PATH,
    }
}

pub(crate) const fn road_asset_path(tier: RoadTier) -> &'static str {
    match tier {
        RoadTier::DirtPath => ROAD_DIRT_PATH,
        RoadTier::Cobblestone => ROAD_COBBLESTONE_PATH,
        RoadTier::Flagstone => ROAD_FLAGSTONE_PATH,
    }
}

pub(crate) fn load_texture(path: &str, context: &str) -> Option<Gd<Texture2D>> {
    let Some(resource) = ResourceLoader::singleton()
        .load_ex(path)
        .type_hint("Texture2D")
        .done()
    else {
        godot_error!("{context}: failed to load texture asset {path}");
        return None;
    };

    match resource.try_cast::<Texture2D>() {
        Ok(texture) => Some(texture),
        Err(resource) => {
            godot_error!(
                "{context}: loaded asset {path} as {}, expected Texture2D",
                resource.get_class()
            );
            None
        }
    }
}

pub(crate) fn load_packed_scene(path: &str, context: &str) -> Option<Gd<PackedScene>> {
    let Some(resource) = ResourceLoader::singleton()
        .load_ex(path)
        .type_hint("PackedScene")
        .done()
    else {
        godot_error!("{context}: failed to load scene asset {path}");
        return None;
    };

    match resource.try_cast::<PackedScene>() {
        Ok(scene) => Some(scene),
        Err(resource) => {
            godot_error!(
                "{context}: loaded asset {path} as {}, expected PackedScene",
                resource.get_class()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_asset_paths_match_generated_assets() {
        assert_eq!(
            terrain_asset_path(TerrainKind::Grass),
            "res://assets/visual/world/terrain/terrain_grass.png"
        );
        assert_eq!(
            terrain_asset_path(TerrainKind::Sand),
            "res://assets/visual/world/terrain/terrain_sand.png"
        );
        assert_eq!(
            terrain_asset_path(TerrainKind::Dirt),
            "res://assets/visual/world/terrain/terrain_dirt.png"
        );
        assert_eq!(
            terrain_asset_path(TerrainKind::Water),
            "res://assets/visual/world/terrain/terrain_water.png"
        );
    }

    #[test]
    fn resource_asset_paths_match_generated_assets() {
        assert_eq!(
            resource_asset_path(ResourceKind::Wood),
            "res://assets/visual/world/resources/resource_wood.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Stone),
            "res://assets/visual/world/resources/resource_stone.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Food),
            "res://assets/visual/world/resources/resource_food.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Gold),
            "res://assets/visual/world/resources/resource_gold.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Crops),
            "res://assets/visual/world/resources/resource_crops.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::WildBerries),
            "res://assets/visual/world/resources/resource_wild_berries.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Planks),
            "res://assets/visual/world/resources/resource_planks.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::StoneBlocks),
            "res://assets/visual/world/resources/resource_stone_blocks.png"
        );
    }

    #[test]
    fn npc_scene_paths_match_world_scenes() {
        assert_eq!(
            npc_scene_path(NpcAppearance::Colonist),
            "res://world/npc_colonist.tscn"
        );
        assert_eq!(
            npc_scene_path(NpcAppearance::Engineer),
            "res://world/npc_engineer.tscn"
        );
        assert_eq!(
            npc_scene_path(NpcAppearance::Botanist),
            "res://world/npc_botanist.tscn"
        );
        assert_eq!(
            npc_scene_path(NpcAppearance::Miner),
            "res://world/npc_miner.tscn"
        );
        assert_eq!(
            npc_scene_path(NpcAppearance::Scout),
            "res://world/npc_scout.tscn"
        );
    }
}
