use crate::world::render_snapshot::{
    BuildingRenderState, DynamicRenderSnapshot, NpcRouteOverlay, PlacementValidity,
    SurfaceRenderSnapshot, WorldOverlaySnapshot,
};
use crate::world::visual::LOGICAL_TILE_SIZE;
use game_engine::buildings::BuildingFootprint;
use game_engine::components::{NpcPosition, SUBTILE_UNITS_PER_TILE};
use game_engine::grid::{CellCoord, GridSize, TILE_SIZE};
use godot::builtin::Side;
use godot::classes::{Camera2D, INode2D, Node2D, TileMapLayer};
use godot::obj::{BaseMut, OnEditor};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = Node2D)]
pub(crate) struct WorldRenderer2D {
    #[export]
    tile_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    resource_node_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    crop_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    tree_plot_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    road_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    road_blueprint_map: OnEditor<Gd<TileMapLayer>>,

    #[export]
    camera: OnEditor<Gd<Camera2D>>,

    surface_size: Option<GridSize>,
    blueprint_footprints: Vec<BuildingFootprint>,
    overlay: WorldOverlaySnapshot,
    base: Base<Node2D>,
}

#[godot_api]
impl INode2D for WorldRenderer2D {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            tile_map: OnEditor::default(),
            resource_node_map: OnEditor::default(),
            crop_map: OnEditor::default(),
            tree_plot_map: OnEditor::default(),
            road_map: OnEditor::default(),
            road_blueprint_map: OnEditor::default(),
            camera: OnEditor::default(),
            surface_size: None,
            blueprint_footprints: Vec::new(),
            overlay: WorldOverlaySnapshot::default(),
            base,
        }
    }

    fn draw(&mut self) {
        let Some(size) = self.surface_size else {
            return;
        };
        let Some(width) = size.width_i32() else {
            godot_warn!("WorldRenderer2D: grid width is too large to draw");
            return;
        };
        let Some(height) = size.height_i32() else {
            godot_warn!("WorldRenderer2D: grid height is too large to draw");
            return;
        };
        let world_size = Vector2::new(width as f32 * TILE_SIZE, height as f32 * TILE_SIZE);
        let grid_color = Color::from_rgba(0.15, 0.24, 0.20, 0.22);
        let overlay = self.overlay.clone();
        let blueprint_footprints = self.blueprint_footprints.clone();
        let mut base = self.base_mut();

        for x in 0..=width {
            let px = x as f32 * TILE_SIZE;
            base.draw_line(
                Vector2::new(px, 0.0),
                Vector2::new(px, world_size.y),
                grid_color,
            );
        }
        for y in 0..=height {
            let py = y as f32 * TILE_SIZE;
            base.draw_line(
                Vector2::new(0.0, py),
                Vector2::new(world_size.x, py),
                grid_color,
            );
        }
        base.draw_rect_ex(
            Rect2::new(Vector2::ZERO, world_size),
            Color::from_rgb(0.95, 0.35, 0.05),
        )
        .filled(false)
        .width(8.0)
        .done();

        for footprint in blueprint_footprints {
            draw_footprint(
                &mut base,
                footprint,
                Color::from_rgb(0.15, 0.85, 1.0),
                0.14,
                3.0,
            );
        }

        for preview in overlay.road_cells {
            draw_cell(
                &mut base,
                preview.coord,
                validity_color(preview.validity),
                0.22,
                4.0,
            );
        }

        if let Some(route) = overlay.selected_npc_route {
            draw_route_overlay(&mut base, route);
        }

        if let Some(selected) = overlay.selected_cell {
            draw_cell(
                &mut base,
                selected.coord,
                Color::from_rgb(1.0, 0.84, 0.0),
                0.15,
                4.0,
            );
        }
        if let Some(selected) = overlay.selected_npc {
            draw_cell(
                &mut base,
                selected.position.coord,
                Color::from_rgb(0.1, 0.85, 1.0),
                0.12,
                4.0,
            );
        }
        if let Some(selected) = overlay.selected_building {
            draw_footprint(
                &mut base,
                selected.footprint,
                Color::from_rgb(1.0, 0.55, 0.12),
                0.10,
                4.0,
            );
        }
        for preview in overlay.plot_cells {
            draw_cell(
                &mut base,
                preview.coord,
                validity_color(preview.validity),
                0.18,
                4.0,
            );
        }
        if let Some(preview) = overlay.building_preview {
            draw_footprint(
                &mut base,
                preview.footprint,
                validity_color(preview.validity),
                0.18,
                4.0,
            );
        }
    }
}

impl WorldRenderer2D {
    pub(crate) fn apply_surface_snapshot(&mut self, snapshot: &SurfaceRenderSnapshot) {
        self.surface_size = Some(snapshot.size);
        self.base_mut().queue_redraw();
    }

    pub(crate) fn apply_dynamic_snapshot(&mut self, snapshot: &DynamicRenderSnapshot) {
        self.blueprint_footprints = snapshot
            .buildings
            .iter()
            .filter(|building| building.state == BuildingRenderState::Blueprint)
            .map(|building| building.footprint)
            .collect();
        self.base_mut().queue_redraw();
    }

    pub(crate) fn apply_overlay_snapshot(&mut self, snapshot: &WorldOverlaySnapshot) {
        if self.overlay != *snapshot {
            self.overlay = snapshot.clone();
            self.base_mut().queue_redraw();
        }
    }

    pub(crate) fn tile_map(&self) -> Gd<TileMapLayer> {
        self.tile_map.clone()
    }

    pub(crate) fn resource_node_map(&self) -> Gd<TileMapLayer> {
        self.resource_node_map.clone()
    }

    pub(crate) fn crop_map(&self) -> Gd<TileMapLayer> {
        self.crop_map.clone()
    }

    pub(crate) fn tree_plot_map(&self) -> Gd<TileMapLayer> {
        self.tree_plot_map.clone()
    }

    pub(crate) fn road_map(&self) -> Gd<TileMapLayer> {
        self.road_map.clone()
    }

    pub(crate) fn road_blueprint_map(&self) -> Gd<TileMapLayer> {
        self.road_blueprint_map.clone()
    }

    pub(crate) fn camera(&self) -> Gd<Camera2D> {
        self.camera.clone()
    }

    pub(crate) fn local_mouse_position(&self) -> Vector2 {
        self.base().get_local_mouse_position()
    }

    pub(crate) fn focus_tiles(&self) -> Vector2 {
        self.camera.get_position() / LOGICAL_TILE_SIZE as f32
    }

    pub(crate) fn set_focus_tiles(&mut self, focus: Vector2) {
        self.camera.set_position(focus * LOGICAL_TILE_SIZE as f32);
    }

    pub(crate) fn configure_for_world_size(&mut self, world_size: Vector2) {
        let padding = world_size;
        let mut camera = self.camera.clone();
        camera.set_position(world_size / 2.0);
        camera.set_limit(Side::LEFT, world_limit(-padding.x));
        camera.set_limit(Side::TOP, world_limit(-padding.y));
        camera.set_limit(Side::RIGHT, world_limit(world_size.x + padding.x));
        camera.set_limit(Side::BOTTOM, world_limit(world_size.y + padding.y));
        camera.set_limit_smoothing_enabled(false);
        camera.set_position_smoothing_enabled(false);
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        self.base_mut().set_visible(active);
        self.camera.set_enabled(active);
        if active {
            self.camera.make_current();
        }
    }

    pub(crate) fn queue_overlay_redraw(&mut self) {
        self.base_mut().queue_redraw();
    }
}

fn world_limit(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }

    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn validity_color(validity: PlacementValidity) -> Color {
    match validity {
        PlacementValidity::Valid => Color::from_rgb(0.1, 0.9, 0.45),
        PlacementValidity::Invalid => Color::from_rgb(1.0, 0.1, 0.1),
    }
}

fn cell_rect(coord: CellCoord) -> Rect2 {
    Rect2::new(
        Vector2::new(coord.x() as f32 * TILE_SIZE, coord.y() as f32 * TILE_SIZE),
        Vector2::new(TILE_SIZE, TILE_SIZE),
    )
}

fn footprint_rect(footprint: BuildingFootprint) -> Rect2 {
    let origin = footprint.origin();
    Rect2::new(
        Vector2::new(origin.x() as f32 * TILE_SIZE, origin.y() as f32 * TILE_SIZE),
        Vector2::new(
            footprint.width() as f32 * TILE_SIZE,
            footprint.height() as f32 * TILE_SIZE,
        ),
    )
}

fn draw_cell(
    base: &mut BaseMut<'_, WorldRenderer2D>,
    coord: CellCoord,
    color: Color,
    fill_alpha: f32,
    width: f32,
) {
    let mut fill = color;
    fill.a = fill_alpha;
    let rect = cell_rect(coord);
    base.draw_rect_ex(rect, fill).filled(true).done();
    base.draw_rect_ex(rect, color)
        .filled(false)
        .width(width)
        .done();
}

fn draw_footprint(
    base: &mut BaseMut<'_, WorldRenderer2D>,
    footprint: BuildingFootprint,
    color: Color,
    fill_alpha: f32,
    width: f32,
) {
    let mut fill = color;
    fill.a = fill_alpha;
    let rect = footprint_rect(footprint);
    base.draw_rect_ex(rect, fill).filled(true).done();
    base.draw_rect_ex(rect, color)
        .filled(false)
        .width(width)
        .done();
}

fn draw_route_overlay(base: &mut BaseMut<'_, WorldRenderer2D>, overlay: NpcRouteOverlay) {
    match overlay {
        NpcRouteOverlay::Route {
            position,
            waypoints,
            destination,
        } => {
            let route_color = Color::from_rgba(0.1, 0.85, 1.0, 0.9);
            let points = npc_route_points(position, &waypoints);
            if points.len() >= 2 {
                let polyline = PackedVector2Array::from(points.clone());
                base.draw_polyline_ex(&polyline, route_color)
                    .width(3.0)
                    .antialiased(true)
                    .done();
                for segment in points.windows(2) {
                    let Some(chevron) = route_chevron(segment[0], segment[1]) else {
                        continue;
                    };
                    let chevron = PackedVector2Array::from(chevron.to_vec());
                    base.draw_polyline_ex(&chevron, route_color)
                        .width(2.0)
                        .antialiased(true)
                        .done();
                }
            }
            base.draw_circle_ex(cell_center(destination), 9.0, route_color)
                .filled(false)
                .width(3.0)
                .antialiased(true)
                .done();
        }
        NpcRouteOverlay::Blocked { position } => {
            let blocked_color = Color::from_rgba(1.0, 0.15, 0.15, 0.95);
            let center = npc_center(position);
            base.draw_circle_ex(center, 13.0, blocked_color)
                .filled(false)
                .width(3.0)
                .antialiased(true)
                .done();
            for offset in [Vector2::new(-8.0, -8.0), Vector2::new(-8.0, 8.0)] {
                base.draw_line_ex(center + offset, center - offset, blocked_color)
                    .width(3.0)
                    .antialiased(true)
                    .done();
            }
        }
    }
}

fn cell_center(coord: CellCoord) -> Vector2 {
    Vector2::new(
        (coord.x() as f32 + 0.5) * TILE_SIZE,
        (coord.y() as f32 + 0.5) * TILE_SIZE,
    )
}

fn npc_center(position: NpcPosition) -> Vector2 {
    cell_center(position.coord)
        + Vector2::new(
            position.subtile_offset.x_units as f32,
            position.subtile_offset.y_units as f32,
        ) * (TILE_SIZE / SUBTILE_UNITS_PER_TILE as f32)
}

fn npc_route_points(position: NpcPosition, waypoints: &[CellCoord]) -> Vec<Vector2> {
    std::iter::once(npc_center(position))
        .chain(waypoints.iter().copied().map(cell_center))
        .collect()
}

fn route_chevron(from: Vector2, to: Vector2) -> Option<[Vector2; 3]> {
    let delta = to - from;
    if delta.length_squared() <= f32::EPSILON {
        return None;
    }
    let direction = delta.normalized();
    let perpendicular = Vector2::new(-direction.y, direction.x);
    let center = from.lerp(to, 0.5);
    let tip = center + direction * 6.0;
    let back = center - direction * 4.0;
    Some([back + perpendicular * 4.0, tip, back - perpendicular * 4.0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::components::SubtileOffset;

    #[test]
    fn world_limits_round_clamp_and_reject_non_finite_values() {
        assert_eq!(world_limit(12.6), 13);
        assert_eq!(world_limit(f32::INFINITY), 0);
        assert_eq!(world_limit(f32::NEG_INFINITY), 0);
    }

    #[test]
    fn route_points_start_at_subtile_npc_center_and_use_cell_centers() {
        let position = NpcPosition {
            coord: CellCoord::new(1, 2),
            subtile_offset: SubtileOffset::new(SUBTILE_UNITS_PER_TILE / 2, 0),
        };

        assert_eq!(
            npc_route_points(position, &[CellCoord::new(2, 2), CellCoord::new(2, 1)]),
            vec![
                Vector2::new(128.0, 160.0),
                Vector2::new(160.0, 160.0),
                Vector2::new(160.0, 96.0),
            ]
        );
    }

    #[test]
    fn route_chevrons_point_along_cardinal_and_diagonal_segments() {
        let center = Vector2::new(32.0, 32.0);
        for direction in [
            Vector2::UP,
            Vector2::new(1.0, -1.0),
            Vector2::RIGHT,
            Vector2::new(1.0, 1.0),
            Vector2::DOWN,
            Vector2::new(-1.0, 1.0),
            Vector2::LEFT,
            Vector2::new(-1.0, -1.0),
        ] {
            let to = center + direction * TILE_SIZE;
            let chevron = route_chevron(center, to).expect("segment should produce a chevron");
            let midpoint = (center + to) * 0.5;
            assert!((chevron[1] - midpoint).dot(direction) > 0.0);
            assert!((chevron[0] - midpoint).dot(direction) < 0.0);
            assert!((chevron[2] - midpoint).dot(direction) < 0.0);
        }
        assert_eq!(route_chevron(center, center), None);
    }

    #[test]
    fn cell_and_footprint_rects_use_the_existing_2d_tile_contract() {
        assert_eq!(
            cell_rect(CellCoord::new(2, 3)),
            Rect2::new(Vector2::new(128.0, 192.0), Vector2::splat(64.0))
        );
        assert_eq!(
            footprint_rect(BuildingFootprint::new(CellCoord::new(2, 3), 4, 2)),
            Rect2::new(Vector2::new(128.0, 192.0), Vector2::new(256.0, 128.0))
        );
    }
}
