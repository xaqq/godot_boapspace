use game_engine::components::{NpcAppearance, TerrainKind};
use game_engine::resources::ResourceKind;
use godot::classes::{PackedScene, ResourceLoader, Texture2D};
use godot::obj::Singleton;
use godot::prelude::*;

const TERRAIN_GRASS_PATH: &str = "res://assets/generated/terrain_grass.png";
const TERRAIN_SAND_PATH: &str = "res://assets/generated/terrain_sand.png";
const TERRAIN_DIRT_PATH: &str = "res://assets/generated/terrain_dirt.png";
const TERRAIN_WATER_PATH: &str = "res://assets/generated/terrain_water.png";
const RESOURCE_WOOD_PATH: &str = "res://assets/generated/resource_wood.png";
const RESOURCE_STONE_PATH: &str = "res://assets/generated/resource_stone.png";
const RESOURCE_FOOD_PATH: &str = "res://assets/generated/resource_food.png";
const RESOURCE_GOLD_PATH: &str = "res://assets/generated/resource_gold.png";
const NPC_COLONIST_SCENE_PATH: &str = "res://world/npc_colonist.tscn";
const NPC_ENGINEER_SCENE_PATH: &str = "res://world/npc_engineer.tscn";
const NPC_BOTANIST_SCENE_PATH: &str = "res://world/npc_botanist.tscn";
const NPC_MINER_SCENE_PATH: &str = "res://world/npc_miner.tscn";
const NPC_SCOUT_SCENE_PATH: &str = "res://world/npc_scout.tscn";

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
            "res://assets/generated/terrain_grass.png"
        );
        assert_eq!(
            terrain_asset_path(TerrainKind::Sand),
            "res://assets/generated/terrain_sand.png"
        );
        assert_eq!(
            terrain_asset_path(TerrainKind::Dirt),
            "res://assets/generated/terrain_dirt.png"
        );
        assert_eq!(
            terrain_asset_path(TerrainKind::Water),
            "res://assets/generated/terrain_water.png"
        );
    }

    #[test]
    fn resource_asset_paths_match_generated_assets() {
        assert_eq!(
            resource_asset_path(ResourceKind::Wood),
            "res://assets/generated/resource_wood.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Stone),
            "res://assets/generated/resource_stone.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Food),
            "res://assets/generated/resource_food.png"
        );
        assert_eq!(
            resource_asset_path(ResourceKind::Gold),
            "res://assets/generated/resource_gold.png"
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
