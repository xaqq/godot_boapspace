use godot::prelude::*;

/// Simulation coordinates remain expressed in 64-pixel logical cells.
pub(crate) const LOGICAL_TILE_SIZE: i32 = 64;

/// New world art is authored at four source pixels per logical pixel.
pub(crate) const AUTHORED_SOURCE_FRAME_SIZE: i32 = 256;
pub(crate) const AUTHORED_RENDER_SCALE: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WorldArtMetrics {
    source_frame_size: i32,
    render_scale: f32,
}

impl WorldArtMetrics {
    pub(crate) const AUTHORED: Self = Self {
        source_frame_size: AUTHORED_SOURCE_FRAME_SIZE,
        render_scale: AUTHORED_RENDER_SCALE,
    };

    pub(crate) fn from_sheet_size(sheet_size: Vector2i, columns: i32, rows: i32) -> Option<Self> {
        if columns <= 0
            || rows <= 0
            || sheet_size.x <= 0
            || sheet_size.y <= 0
            || sheet_size.x % columns != 0
            || sheet_size.y % rows != 0
        {
            return None;
        }

        let frame_width = sheet_size.x / columns;
        let frame_height = sheet_size.y / rows;
        if frame_width != frame_height {
            return None;
        }

        Some(Self {
            source_frame_size: frame_width,
            render_scale: LOGICAL_TILE_SIZE as f32 / frame_width as f32,
        })
    }

    pub(crate) fn from_texture(texture: &Gd<godot::classes::Texture2D>) -> Option<Self> {
        Self::from_sheet_size(
            Vector2i::new(texture.get_width(), texture.get_height()),
            1,
            1,
        )
    }

    pub(crate) const fn source_frame_size(self) -> i32 {
        self.source_frame_size
    }

    pub(crate) const fn render_scale(self) -> f32 {
        self.render_scale
    }

    pub(crate) const fn node_scale(self) -> Vector2 {
        Vector2::new(self.render_scale, self.render_scale)
    }

    pub(crate) fn atlas_region(self, column: i32, row: i32) -> Rect2 {
        let frame = self.source_frame_size as f32;
        Rect2::new(
            Vector2::new(column as f32 * frame, row as f32 * frame),
            Vector2::new(frame, frame),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_metrics_map_256_source_pixels_to_one_logical_cell() {
        assert_eq!(WorldArtMetrics::AUTHORED.source_frame_size(), 256);
        assert_eq!(WorldArtMetrics::AUTHORED.render_scale(), 0.25);
        assert_eq!(
            WorldArtMetrics::AUTHORED.source_frame_size() as f32
                * WorldArtMetrics::AUTHORED.render_scale(),
            LOGICAL_TILE_SIZE as f32
        );
    }

    #[test]
    fn sheet_metrics_support_legacy_and_authored_assets() {
        let legacy = WorldArtMetrics::from_sheet_size(Vector2i::new(256, 512), 4, 8)
            .expect("legacy sheet should be valid");
        let authored = WorldArtMetrics::from_sheet_size(Vector2i::new(1024, 2048), 4, 8)
            .expect("authored sheet should be valid");

        assert_eq!(legacy.source_frame_size(), 64);
        assert_eq!(legacy.render_scale(), 1.0);
        assert_eq!(authored, WorldArtMetrics::AUTHORED);
    }

    #[test]
    fn sheet_metrics_reject_invalid_or_non_square_frames() {
        assert_eq!(
            WorldArtMetrics::from_sheet_size(Vector2i::new(1023, 2048), 4, 8),
            None
        );
        assert_eq!(
            WorldArtMetrics::from_sheet_size(Vector2i::new(1024, 1024), 4, 8),
            None
        );
        assert_eq!(
            WorldArtMetrics::from_sheet_size(Vector2i::new(1024, 2048), 0, 8),
            None
        );
    }

    #[test]
    fn atlas_regions_use_the_inferred_source_frame() {
        assert_eq!(
            WorldArtMetrics::AUTHORED.atlas_region(3, 7),
            Rect2::new(Vector2::new(768.0, 1792.0), Vector2::new(256.0, 256.0))
        );
    }
}
