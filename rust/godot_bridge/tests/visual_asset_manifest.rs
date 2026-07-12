use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use image::{DynamicImage, GenericImageView};
use serde::Deserialize;

const EXPECTED_LEGACY_ASSET_COUNT: usize = 75;
const MAX_TILE_EDGE_MEAN_ERROR: u64 = 24;
const ALLOWED_CATEGORIES: &[&str] = &[
    "world/terrain",
    "world/resources",
    "world/buildings",
    "world/characters",
    "world/vehicles",
    "world/farming",
    "world/roads",
    "world/effects",
    "ui",
    "menu",
];

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    logical_tile_pixels: u32,
    source_tile_pixels: u32,
    render_scale: f32,
    style: String,
    family: Vec<AssetFamily>,
}

#[derive(Debug, Deserialize)]
struct AssetFamily {
    id: String,
    category: String,
    legacy_dir: String,
    target_dir: String,
    files: Vec<String>,
    ready: bool,
    frame_width: u32,
    frame_height: u32,
    columns: u32,
    rows: u32,
    alpha: AlphaRequirement,
    seams: SeamRequirement,
    anchor: [f32; 2],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum AlphaRequirement {
    Opaque,
    Required,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum SeamRequirement {
    None,
    Tileable,
    ConnectedAtlas,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace repository root must exist")
}

fn load_manifest() -> Manifest {
    toml::from_str(include_str!(
        "../../../godot/assets/visual/asset_manifest.toml"
    ))
    .expect("visual asset manifest must be valid TOML")
}

fn expanded_legacy_paths(manifest: &Manifest) -> BTreeSet<String> {
    manifest
        .family
        .iter()
        .flat_map(|family| {
            family
                .files
                .iter()
                .map(|file| format!("{}/{file}", family.legacy_dir))
        })
        .collect()
}

fn png_paths_in(directory: &Path, repository_root: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return BTreeSet::new();
    };

    entries
        .map(|entry| {
            entry
                .expect("legacy asset directory must be readable")
                .path()
        })
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .map(|path| {
            path.strip_prefix(repository_root)
                .expect("asset must be inside repository")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

#[test]
fn manifest_encodes_the_world_art_scale_contract() {
    let manifest = load_manifest();

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.logical_tile_pixels, 64);
    assert_eq!(manifest.source_tile_pixels, 256);
    assert!((manifest.render_scale - 0.25).abs() < f32::EPSILON);
    assert_eq!(
        manifest.source_tile_pixels as f32 * manifest.render_scale,
        manifest.logical_tile_pixels as f32
    );
    assert_eq!(
        manifest.style,
        "painterly-lived-in-sci-fi-frontier-copper-cyan"
    );
}

#[test]
fn manifest_supersedes_every_legacy_generated_png() {
    let manifest = load_manifest();
    let declared = expanded_legacy_paths(&manifest);

    assert_eq!(
        declared.len(),
        EXPECTED_LEGACY_ASSET_COUNT,
        "the migration inventory must keep all 75 original PNGs until the cutover is complete"
    );

    let root = repository_root();
    let on_disk = png_paths_in(&root.join("godot/assets/generated"), &root);
    if !on_disk.is_empty() {
        assert_eq!(
            declared, on_disk,
            "the visual manifest must supersede every PNG in godot/assets/generated"
        );
    }
}

#[test]
fn every_family_has_a_valid_and_unambiguous_contract() {
    let manifest = load_manifest();
    let mut family_ids = BTreeSet::new();
    let mut target_paths = BTreeSet::new();

    for family in &manifest.family {
        assert!(
            family_ids.insert(&family.id),
            "duplicate family {}",
            family.id
        );
        assert!(
            ALLOWED_CATEGORIES.contains(&family.category.as_str()),
            "family {} uses unknown category {}",
            family.id,
            family.category
        );
        assert!(!family.files.is_empty(), "family {} is empty", family.id);
        assert!(family.frame_width > 0 && family.frame_height > 0);
        assert!(family.columns > 0 && family.rows > 0);
        assert_eq!(
            family.frame_width % manifest.source_tile_pixels,
            0,
            "{} frame width must use the 256 px source-tile metric",
            family.id
        );
        assert_eq!(
            family.frame_height % manifest.source_tile_pixels,
            0,
            "{} frame height must use the 256 px source-tile metric",
            family.id
        );
        assert!(
            (0.0..=1.0).contains(&family.anchor[0]) && (0.0..=1.0).contains(&family.anchor[1]),
            "{} anchor must be normalized",
            family.id
        );

        for file in &family.files {
            assert_eq!(
                Path::new(file).file_name().and_then(|name| name.to_str()),
                Some(file.as_str()),
                "family file names cannot contain directories"
            );
            assert!(file.ends_with(".png"), "{file} must be a PNG");
            let target = format!("{}/{file}", family.target_dir);
            assert!(
                target_paths.insert(target.clone()),
                "duplicate target {target}"
            );
        }
    }
}

#[test]
fn ready_asset_families_match_dimensions_alpha_frames_and_seams() {
    validate_assets(false);
}

#[test]
#[ignore = "run after all manifest families have been generated"]
fn complete_visual_manifest_matches_dimensions_alpha_frames_and_seams() {
    validate_assets(true);
}

fn validate_assets(require_all: bool) {
    let root = repository_root();
    let manifest = load_manifest();

    for family in &manifest.family {
        if !require_all && !family.ready {
            continue;
        }

        for file in &family.files {
            let path = root.join(&family.target_dir).join(file);
            assert!(
                path.is_file(),
                "ready family {} is missing {}",
                family.id,
                path.display()
            );
            let image = image::open(&path)
                .unwrap_or_else(|error| panic!("cannot decode {}: {error}", path.display()));
            validate_image(&image, family, &path);
        }
    }
}

fn validate_image(image: &DynamicImage, family: &AssetFamily, path: &Path) {
    let expected_width = family.frame_width * family.columns;
    let expected_height = family.frame_height * family.rows;
    assert_eq!(
        image.dimensions(),
        (expected_width, expected_height),
        "{} does not match the {} frame grid",
        path.display(),
        family.id
    );

    let has_alpha_channel = image.color().has_alpha();
    let rgba = image.to_rgba8();
    match family.alpha {
        AlphaRequirement::Opaque => assert!(
            rgba.pixels().all(|pixel| pixel[3] == u8::MAX),
            "{} must be fully opaque",
            path.display()
        ),
        AlphaRequirement::Required => {
            assert!(
                has_alpha_channel,
                "{} must contain an alpha channel",
                path.display()
            );
            assert!(
                rgba.pixels().any(|pixel| pixel[3] < u8::MAX),
                "{} must contain transparent background pixels",
                path.display()
            );
        }
    }

    for row in 0..family.rows {
        for column in 0..family.columns {
            let x = column * family.frame_width;
            let y = row * family.frame_height;
            let frame = image.crop_imm(x, y, family.frame_width, family.frame_height);
            assert!(
                frame.to_rgba8().pixels().any(|pixel| pixel[3] != 0),
                "{} contains an empty frame at ({column}, {row})",
                path.display()
            );

            if family.seams == SeamRequirement::Tileable {
                validate_tile_edges(&frame, path, column, row);
            }
        }
    }
}

fn validate_tile_edges(image: &DynamicImage, path: &Path, column: u32, row: u32) {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    let horizontal_error = (0..height)
        .map(|y| pixel_rgb_error(rgba.get_pixel(0, y), rgba.get_pixel(width - 1, y)))
        .sum::<u64>()
        / (height as u64 * 3);
    let vertical_error = (0..width)
        .map(|x| pixel_rgb_error(rgba.get_pixel(x, 0), rgba.get_pixel(x, height - 1)))
        .sum::<u64>()
        / (width as u64 * 3);

    assert!(
        horizontal_error <= MAX_TILE_EDGE_MEAN_ERROR,
        "{} frame ({column}, {row}) left/right mean seam error {horizontal_error} exceeds {MAX_TILE_EDGE_MEAN_ERROR}",
        path.display()
    );
    assert!(
        vertical_error <= MAX_TILE_EDGE_MEAN_ERROR,
        "{} frame ({column}, {row}) top/bottom mean seam error {vertical_error} exceeds {MAX_TILE_EDGE_MEAN_ERROR}",
        path.display()
    );
}

fn pixel_rgb_error(left: &image::Rgba<u8>, right: &image::Rgba<u8>) -> u64 {
    (0..3)
        .map(|channel| left[channel].abs_diff(right[channel]) as u64)
        .sum()
}
