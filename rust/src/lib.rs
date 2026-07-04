use godot::prelude::*;

mod game_surface;
mod game_world;
mod ingame_menu;
mod resource_header;
mod resources;
mod root_menu;
mod selected_tile;
mod tile_info_panel;

struct GodotBoapspaceExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotBoapspaceExtension {}
