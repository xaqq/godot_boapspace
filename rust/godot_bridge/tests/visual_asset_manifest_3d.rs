use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use game_engine::buildings::BuildingKind;
use game_engine::components::NpcAppearance;
use game_engine::resources::ResourceKind;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const EXPECTED_BLENDER_VERSION: &str = "5.0.1";
const EXPECTED_GODOT_VERSION: &str = "4.7.stable.official.5b4e0cb0f";
const EXPECTED_NPC_ANIMATIONS: &[&str] = &[
    "idle",
    "walk",
    "gather",
    "saw",
    "stonecut",
    "cook",
    "carry_idle",
    "carry_walk",
    "wheelbarrow_idle",
    "wheelbarrow_walk",
];

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    blender_version: String,
    godot_version: String,
    generator: String,
    source_manifest: String,
    tile_units: f64,
    subtile_units_per_tile: u32,
    up_axis: String,
    forward_axis: String,
    animation_fps: u32,
    ready: bool,
    sources: SourceHashes,
    skeleton: SkeletonContract,
    material: Vec<MaterialEntry>,
    model: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct SourceHashes {
    model_turnaround_sha256: String,
    material_source_sha256: String,
}

#[derive(Debug, Deserialize)]
struct SkeletonContract {
    id: String,
    bones: Vec<String>,
    parents: Vec<String>,
    animations: Vec<String>,
    fps: u32,
    stationary_root: bool,
}

#[derive(Debug, Deserialize)]
struct MaterialEntry {
    id: String,
    base_color: String,
    normal: String,
    orm: String,
    base_color_sha256: String,
    normal_sha256: String,
    orm_sha256: String,
    width: u32,
    height: u32,
    ready: bool,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    family: String,
    variant: String,
    path: String,
    footprint: [u32; 2],
    materials: Vec<String>,
    triangle_budget: usize,
    triangle_count: usize,
    pivot: [f64; 3],
    bounds_min: [f64; 3],
    bounds_max: [f64; 3],
    sha256: String,
    provenance_sha256: String,
    source_ids: Vec<String>,
    #[serde(default)]
    animations: Vec<String>,
    skeleton: Option<String>,
    wrapper: Option<String>,
    ready: bool,
}

#[derive(Debug, Deserialize)]
struct SourceManifest {
    schema_version: u32,
    source: Vec<SourceEntry>,
}

#[derive(Debug, Deserialize)]
struct SourceEntry {
    id: String,
    path: String,
    role: String,
    prompt: String,
    inputs: Vec<String>,
    input_sha256: Vec<String>,
    selected_output: String,
    sha256: String,
    ready: bool,
}

struct Glb {
    document: Value,
    binary: Vec<u8>,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace repository root must exist")
}

fn load_manifest(root: &Path) -> Manifest {
    let path = root.join("godot/assets/visual/asset_manifest_3d.toml");
    toml::from_str(&fs::read_to_string(path).expect("3D asset manifest must be readable"))
        .expect("3D asset manifest must be valid TOML")
}

fn sha256(path: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(
        fs::read(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display())),
    );
    let digest = digest.finalize();
    format!("{digest:x}")
}

fn parse_glb(path: &Path) -> Glb {
    let bytes =
        fs::read(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    assert!(bytes.len() >= 28, "{} is too short for GLB", path.display());
    assert_eq!(
        &bytes[0..4],
        b"glTF",
        "{} has invalid magic",
        path.display()
    );
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
        bytes.len(),
        "{} has invalid declared length",
        path.display()
    );

    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    assert_eq!(
        u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        0x4e4f_534a
    );
    let json_end = 20 + json_length;
    let json_bytes = &bytes[20..json_end];
    let document = serde_json::from_slice(json_bytes).expect("GLB JSON chunk must be valid JSON");
    let binary_length =
        u32::from_le_bytes(bytes[json_end..json_end + 4].try_into().unwrap()) as usize;
    assert_eq!(
        u32::from_le_bytes(bytes[json_end + 4..json_end + 8].try_into().unwrap()),
        0x004e_4942
    );
    let binary_start = json_end + 8;
    let binary = bytes[binary_start..binary_start + binary_length].to_vec();
    Glb { document, binary }
}

fn value_array<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("GLB JSON must contain array {key}"))
}

fn accessor_values(glb: &Glb, accessor_index: usize) -> Vec<Vec<f64>> {
    let accessor = &value_array(&glb.document, "accessors")[accessor_index];
    assert!(
        accessor.get("sparse").is_none(),
        "generated GLBs cannot use sparse accessors"
    );
    let view_index = accessor["bufferView"].as_u64().unwrap() as usize;
    let view = &value_array(&glb.document, "bufferViews")[view_index];
    let component_type = accessor["componentType"].as_u64().unwrap() as u32;
    let component_size = match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => panic!("unsupported accessor component type {component_type}"),
    };
    let component_count = match accessor["type"].as_str().unwrap() {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        kind => panic!("unsupported accessor type {kind}"),
    };
    let count = accessor["count"].as_u64().unwrap() as usize;
    let packed_size = component_size * component_count;
    let stride = view
        .get("byteStride")
        .and_then(Value::as_u64)
        .map_or(packed_size, |value| value as usize);
    let offset = view
        .get("byteOffset")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize
        + accessor
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
    let normalized = accessor
        .get("normalized")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    (0..count)
        .map(|index| {
            let start = offset + index * stride;
            (0..component_count)
                .map(|component| {
                    let cursor = start + component * component_size;
                    let raw =
                        match component_type {
                            5120 => i8::from_le_bytes([glb.binary[cursor]]) as f64,
                            5121 => glb.binary[cursor] as f64,
                            5122 => i16::from_le_bytes(
                                glb.binary[cursor..cursor + 2].try_into().unwrap(),
                            ) as f64,
                            5123 => u16::from_le_bytes(
                                glb.binary[cursor..cursor + 2].try_into().unwrap(),
                            ) as f64,
                            5125 => u32::from_le_bytes(
                                glb.binary[cursor..cursor + 4].try_into().unwrap(),
                            ) as f64,
                            5126 => f32::from_le_bytes(
                                glb.binary[cursor..cursor + 4].try_into().unwrap(),
                            ) as f64,
                            _ => unreachable!(),
                        };
                    if !normalized || component_type == 5126 {
                        raw
                    } else {
                        match component_type {
                            5120 => (raw / 127.0).max(-1.0),
                            5121 => raw / 255.0,
                            5122 => (raw / 32767.0).max(-1.0),
                            5123 => raw / 65535.0,
                            5125 => raw / 4_294_967_295.0,
                            _ => unreachable!(),
                        }
                    }
                })
                .collect()
        })
        .collect()
}

fn triangle_count(glb: &Glb) -> usize {
    value_array(&glb.document, "meshes")
        .iter()
        .flat_map(|mesh| value_array(mesh, "primitives"))
        .map(|primitive| {
            assert_eq!(
                primitive.get("mode").and_then(Value::as_u64).unwrap_or(4),
                4,
                "all generated geometry must use triangle primitives"
            );
            let count = if let Some(index) = primitive.get("indices").and_then(Value::as_u64) {
                value_array(&glb.document, "accessors")[index as usize]["count"]
                    .as_u64()
                    .unwrap() as usize
            } else {
                let position = primitive["attributes"]["POSITION"].as_u64().unwrap() as usize;
                value_array(&glb.document, "accessors")[position]["count"]
                    .as_u64()
                    .unwrap() as usize
            };
            assert_eq!(count % 3, 0);
            count / 3
        })
        .sum()
}

fn material_names(glb: &Glb) -> BTreeSet<String> {
    value_array(&glb.document, "materials")
        .iter()
        .map(|material| material["name"].as_str().unwrap().to_owned())
        .collect()
}

fn animation_names(glb: &Glb) -> BTreeSet<String> {
    glb.document
        .get("animations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|animation| animation["name"].as_str().unwrap().to_owned())
        .collect()
}

fn skin_hierarchy(glb: &Glb) -> Vec<(String, String)> {
    let skins = value_array(&glb.document, "skins");
    assert_eq!(skins.len(), 1, "NPC GLB must have one skin");
    let joints = skins[0]["joints"].as_array().unwrap();
    let joint_indices = joints
        .iter()
        .map(|value| value.as_u64().unwrap() as usize)
        .collect::<BTreeSet<_>>();
    let nodes = value_array(&glb.document, "nodes");
    let mut parents = BTreeMap::new();
    for (parent, node) in nodes.iter().enumerate() {
        for child in node
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            assert!(
                parents
                    .insert(child.as_u64().unwrap() as usize, parent)
                    .is_none(),
                "GLB node cannot have multiple parents"
            );
        }
    }
    joints
        .iter()
        .map(|joint| {
            let index = joint.as_u64().unwrap() as usize;
            let name = nodes[index]["name"].as_str().unwrap().to_owned();
            let parent = parents
                .get(&index)
                .filter(|parent| joint_indices.contains(parent))
                .map_or("", |parent| nodes[*parent]["name"].as_str().unwrap())
                .to_owned();
            (name, parent)
        })
        .collect()
}

fn validate_weights(glb: &Glb) {
    let joint_count = value_array(&glb.document, "skins")[0]["joints"]
        .as_array()
        .unwrap()
        .len();
    let mut vertex_count = 0;
    for primitive in value_array(&glb.document, "meshes")
        .iter()
        .flat_map(|mesh| value_array(mesh, "primitives"))
    {
        let attributes = primitive["attributes"].as_object().unwrap();
        let joints = accessor_values(glb, attributes["JOINTS_0"].as_u64().unwrap() as usize);
        let weights = accessor_values(glb, attributes["WEIGHTS_0"].as_u64().unwrap() as usize);
        assert_eq!(joints.len(), weights.len());
        for (joints, weights) in joints.iter().zip(&weights) {
            assert!(joints
                .iter()
                .all(|joint| *joint >= 0.0 && (*joint as usize) < joint_count));
            assert!(weights.iter().all(|weight| *weight >= -1.0e-6));
            assert!((weights.iter().sum::<f64>() - 1.0).abs() <= 1.0e-3);
            assert!(weights.iter().filter(|weight| **weight > 1.0e-6).count() <= 4);
            vertex_count += 1;
        }
    }
    assert!(vertex_count > 0, "NPC skin must contain weighted vertices");
}

fn validate_animation_sampling_and_stationary_root(glb: &Glb) {
    let nodes = value_array(&glb.document, "nodes");
    let root_nodes = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| (node["name"].as_str() == Some("Root")).then_some(index))
        .collect::<BTreeSet<_>>();
    for animation in value_array(&glb.document, "animations") {
        let samplers = value_array(animation, "samplers");
        for sampler in samplers {
            let times = accessor_values(glb, sampler["input"].as_u64().unwrap() as usize);
            for samples in times.windows(2) {
                let frames = (samples[1][0] - samples[0][0]) * 30.0;
                assert!((frames - frames.round()).abs() <= 2.0e-4);
            }
        }
        for channel in value_array(animation, "channels") {
            let target = &channel["target"];
            let Some(node) = target.get("node").and_then(Value::as_u64) else {
                continue;
            };
            if target["path"].as_str() != Some("translation")
                || !root_nodes.contains(&(node as usize))
            {
                continue;
            }
            let sampler = &samplers[channel["sampler"].as_u64().unwrap() as usize];
            let values = accessor_values(glb, sampler["output"].as_u64().unwrap() as usize);
            assert!(!values.is_empty());
            assert!(values.iter().all(|value| {
                value
                    .iter()
                    .zip(&values[0])
                    .all(|(component, initial)| (component - initial).abs() <= 1.0e-6)
            }));
        }
    }
}

#[test]
fn generated_3d_manifest_covers_every_enum_and_asset_contract() {
    let root = repository_root();
    let manifest = load_manifest(&root);
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.blender_version, EXPECTED_BLENDER_VERSION);
    assert_eq!(manifest.godot_version, EXPECTED_GODOT_VERSION);
    assert_eq!(manifest.generator, "tools/assets_3d/generate.py");
    assert_eq!(
        manifest.source_manifest,
        "art_sources/world_3d/source_manifest.toml"
    );
    assert_eq!(manifest.tile_units, 2.0);
    assert_eq!(manifest.subtile_units_per_tile, 1024);
    assert_eq!(manifest.up_axis, "+Y");
    assert_eq!(manifest.forward_axis, "-Z");
    assert_eq!(manifest.animation_fps, 30);
    assert!(manifest.ready);
    assert_eq!(manifest.model.len(), 35);

    let expected_buildings = BuildingKind::ALL
        .map(|kind| format!("{kind:?}"))
        .into_iter()
        .collect::<BTreeSet<_>>();
    let expected_resources = ResourceKind::ALL
        .map(|kind| format!("{kind:?}"))
        .into_iter()
        .collect::<BTreeSet<_>>();
    let expected_npcs = NpcAppearance::ALL
        .map(|appearance| format!("{appearance:?}"))
        .into_iter()
        .collect::<BTreeSet<_>>();
    let variants = |family: &str| {
        manifest
            .model
            .iter()
            .filter(|model| model.family == family)
            .map(|model| model.variant.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(variants("building"), expected_buildings);
    assert_eq!(variants("resource"), expected_resources);
    assert_eq!(variants("npc"), expected_npcs);
    assert_eq!(
        variants("crop"),
        ["Seedable", "GrowingStep1", "GrowingStep2", "Grown"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert_eq!(
        variants("tree"),
        ["Sapling", "Young", "Mature"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert_eq!(
        variants("wheelbarrow"),
        BTreeSet::from(["Wheelbarrow".to_owned()])
    );
    assert_eq!(variants("props"), BTreeSet::from(["WorkProps".to_owned()]));

    assert_eq!(manifest.skeleton.id, "boapspace_humanoid_v1");
    assert_eq!(manifest.skeleton.bones.len(), 18);
    assert_eq!(
        manifest.skeleton.bones.len(),
        manifest.skeleton.parents.len()
    );
    assert_eq!(manifest.skeleton.animations, EXPECTED_NPC_ANIMATIONS);
    assert_eq!(manifest.skeleton.fps, 30);
    assert!(manifest.skeleton.stationary_root);
}

#[test]
fn generated_materials_sources_hashes_and_provenance_are_complete() {
    let root = repository_root();
    let manifest = load_manifest(&root);
    let expected_materials =
        BTreeSet::from(["structure", "timber", "masonry_ore", "organic", "character"]);
    assert_eq!(
        manifest
            .material
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<BTreeSet<_>>(),
        expected_materials
    );
    for material in &manifest.material {
        assert!(material.ready);
        assert_eq!((material.width, material.height), (256, 256));
        for (path, expected_hash) in [
            (&material.base_color, &material.base_color_sha256),
            (&material.normal, &material.normal_sha256),
            (&material.orm, &material.orm_sha256),
        ] {
            assert_eq!(sha256(&root.join(path)), *expected_hash);
            let import = fs::read_to_string(root.join(format!("{path}.import"))).unwrap();
            assert!(import.contains("compress/mode=2"));
            assert!(import.contains("compress/high_quality=true"));
            assert!(import.contains("mipmaps/generate=true"));
            let expected_normal = if path == &material.normal { 1 } else { 0 };
            assert!(import.contains(&format!("compress/normal_map={expected_normal}")));
        }
    }

    let source_path = root.join(&manifest.source_manifest);
    let source_manifest: SourceManifest =
        toml::from_str(&fs::read_to_string(source_path).unwrap()).unwrap();
    assert_eq!(source_manifest.schema_version, 1);
    assert_eq!(source_manifest.source.len(), 2);
    let source_hashes = BTreeMap::from([
        (
            "model_turnaround",
            manifest.sources.model_turnaround_sha256.as_str(),
        ),
        (
            "material_source",
            manifest.sources.material_source_sha256.as_str(),
        ),
    ]);
    for source in source_manifest.source {
        assert!(source.ready);
        assert!(!source.role.is_empty() && !source.prompt.is_empty() && !source.inputs.is_empty());
        assert_eq!(source.inputs.len(), source.input_sha256.len());
        for (input, expected_hash) in source.inputs.iter().zip(&source.input_sha256) {
            assert_eq!(sha256(&root.join(input)), *expected_hash);
        }
        assert_eq!(source.path, source.selected_output);
        assert_eq!(source_hashes[source.id.as_str()], source.sha256);
        assert_eq!(sha256(&root.join(source.path)), source.sha256);
    }
    for model in &manifest.model {
        assert_eq!(model.source_ids, ["model_turnaround", "material_source"]);
        assert_eq!(model.provenance_sha256.len(), 64);
        assert!(model
            .provenance_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
    }
}

#[test]
fn every_glb_obeys_budgets_materials_pivots_skins_and_animation_contracts() {
    let root = repository_root();
    let manifest = load_manifest(&root);
    let expected_hierarchy = manifest
        .skeleton
        .bones
        .iter()
        .cloned()
        .zip(manifest.skeleton.parents.iter().cloned())
        .collect::<Vec<_>>();
    let expected_npc_animations = EXPECTED_NPC_ANIMATIONS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();

    for model in &manifest.model {
        assert!(model.ready);
        assert!(model.footprint.into_iter().all(|size| size > 0));
        assert_eq!(model.pivot, [0.0, 0.0, 0.0]);
        assert!(model
            .bounds_min
            .into_iter()
            .chain(model.bounds_max)
            .all(f64::is_finite));
        assert!(model.bounds_min[1] >= -0.03);
        assert!(model.triangle_count > 0 && model.triangle_count <= model.triangle_budget);
        let path = root.join(&model.path);
        assert_eq!(sha256(&path), model.sha256);
        let glb = parse_glb(&path);
        assert_eq!(triangle_count(&glb), model.triangle_count);
        assert_eq!(
            material_names(&glb),
            model.materials.iter().cloned().collect()
        );
        let expected_images = model
            .materials
            .iter()
            .flat_map(|material| {
                ["base_color", "normal", "orm"]
                    .map(|role| format!("../materials/{material}_{role}.png"))
            })
            .collect::<BTreeSet<_>>();
        let images = value_array(&glb.document, "images");
        assert_eq!(
            images
                .iter()
                .map(|image| image["uri"].as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>(),
            expected_images
        );
        assert!(images.iter().all(|image| image.get("bufferView").is_none()));
        assert_eq!(
            animation_names(&glb),
            model.animations.iter().cloned().collect()
        );

        match model.family.as_str() {
            "npc" => {
                assert_eq!(
                    model.skeleton.as_deref(),
                    Some(manifest.skeleton.id.as_str())
                );
                assert_eq!(skin_hierarchy(&glb), expected_hierarchy);
                assert_eq!(animation_names(&glb), expected_npc_animations);
                validate_weights(&glb);
                validate_animation_sampling_and_stationary_root(&glb);
            }
            "wheelbarrow" => {
                assert_eq!(
                    animation_names(&glb),
                    BTreeSet::from(["idle".to_owned(), "roll".to_owned()])
                );
                validate_animation_sampling_and_stationary_root(&glb);
            }
            _ => assert!(model.animations.is_empty()),
        }
    }
}

#[test]
fn typed_wrappers_and_headless_gallery_cover_every_model() {
    let root = repository_root();
    let manifest = load_manifest(&root);
    let gallery = fs::read_to_string(root.join("godot/world/3d/asset_gallery.tscn")).unwrap();
    let smoke = fs::read_to_string(root.join("godot/world/3d/asset_smoke_test.tscn")).unwrap();
    assert!(smoke.contains("res://world/3d/asset_gallery.tscn"));
    assert!(smoke.contains("type=\"AssetSmokeTest3D\""));
    for (field, node) in [
        ("colonist", "Colonist"),
        ("engineer", "Engineer"),
        ("botanist", "Botanist"),
        ("miner", "Miner"),
        ("scout", "Scout"),
        ("wheelbarrow", "Wheelbarrow"),
    ] {
        assert!(smoke.contains(&format!("{field} = NodePath(\"Gallery/{node}\")")));
    }

    for model in &manifest.model {
        let expected_path = model.wrapper.as_deref().unwrap_or(&model.path);
        assert!(
            gallery.contains(
                expected_path
                    .strip_prefix("godot/")
                    .unwrap_or(expected_path)
            ),
            "gallery does not instance {}",
            model.id
        );
        let Some(wrapper) = &model.wrapper else {
            continue;
        };
        let contents = fs::read_to_string(root.join(wrapper)).unwrap();
        assert!(contents.contains(model.path.strip_prefix("godot/").unwrap_or(&model.path)));
        assert!(
            !contents.contains("script ="),
            "3D wrappers must not use GDScript"
        );
        if model.family == "npc" {
            for node in [
                "RightHandAttachment",
                "LeftHandAttachment",
                "CarryAttachment",
                "WheelbarrowAttachment",
            ] {
                assert!(contents.contains(&format!("name=\"{node}\"")));
            }
            assert_eq!(contents.matches("type=\"BoneAttachment3D\"").count(), 4);
            assert_eq!(contents.matches("use_external_skeleton = true").count(), 4);
        }
    }
}
