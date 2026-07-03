use godot::prelude::*;

mod game_surface;
mod game_world;
mod ingame_menu;
mod resource_header;
mod resources;
mod root_menu;

struct GodotBoapspaceExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotBoapspaceExtension {}
