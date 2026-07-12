use godot::prelude::*;

mod assets;
mod entity_id;

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
    mod task_table_view;
    mod tile_info_panel;
}

mod world {
    pub(crate) mod asset_smoke_test_3d;
    pub(crate) mod game_world;
    pub(crate) mod mesh_builder_3d;
    pub(crate) mod model_library_3d;
    pub(crate) mod model_wrapper_3d;
    pub(crate) mod render_snapshot;
    pub(crate) mod visual;
    pub(crate) mod world_renderer_2d;
    pub(crate) mod world_renderer_3d;
    pub(crate) mod world_renderer_smoke_test;
}

struct GodotBoapspaceExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotBoapspaceExtension {}
