use godot::prelude::*;

mod assets;

mod menu {
    mod ingame_menu;
    mod root_menu;
}

mod panel {
    mod building_info_panel;
    mod building_palette;
    mod npc_info_panel;
    mod resource_quantity;
    mod resource_quantity_progress;
    mod resource_tooltip;
    mod simulation_header_bar;
    mod surface_selector;
    mod task_list_panel;
    mod tile_info_panel;
}

mod world {
    pub(crate) mod game_world;
}

struct GodotBoapspaceExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotBoapspaceExtension {}
