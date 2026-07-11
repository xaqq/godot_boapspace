use godot::prelude::*;

mod assets;

mod menu {
    mod ingame_menu;
    mod root_menu;
}

mod panel {
    mod building_info_panel;
    pub(crate) mod construction_dock;
    mod housing_panel;
    mod map_entity_tooltip_panel;
    mod npc_details;
    mod npc_info_panel;
    mod performance_info;
    mod resource_history_graph;
    mod resource_panel;
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
