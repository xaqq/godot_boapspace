use godot::prelude::*;

mod game_world;
mod ingame_menu;
mod resource_header;
mod root_menu;
mod surface_selector;
mod tile_info_panel;

struct GodotBoapspaceExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotBoapspaceExtension {}
