use godot::prelude::*;

mod building_info_panel;
mod building_palette;
mod game_world;
mod ingame_menu;
mod npc_info_panel;
mod resource_header;
mod root_menu;
mod surface_selector;
mod tile_info_panel;

struct GodotBoapspaceExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotBoapspaceExtension {}
