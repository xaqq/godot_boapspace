use crate::game_world::GameWorld;
use game_engine::resources::{GameResources, ResourceKind};
use godot::classes::{
    image, texture_rect, HBoxContainer, IHBoxContainer, Image, ImageTexture, Label, Texture2D,
    TextureRect,
};
use godot::obj::OnEditor;
use godot::prelude::*;

type ResourceLabels = (Gd<Label>, Gd<Label>, Gd<Label>, Gd<Label>);
type ResourceIcons = (
    Gd<TextureRect>,
    Gd<TextureRect>,
    Gd<TextureRect>,
    Gd<TextureRect>,
);

const RESOURCE_WOOD_PATH: &str = "res://assets/generated/resource_wood.png";
const RESOURCE_STONE_PATH: &str = "res://assets/generated/resource_stone.png";
const RESOURCE_FOOD_PATH: &str = "res://assets/generated/resource_food.png";
const RESOURCE_GOLD_PATH: &str = "res://assets/generated/resource_gold.png";

#[derive(GodotClass)]
#[class(base = HBoxContainer)]
pub(crate) struct ResourceHeader {
    #[export]
    wood_icon: OnEditor<Gd<TextureRect>>,

    #[export]
    wood_label: OnEditor<Gd<Label>>,

    #[export]
    stone_icon: OnEditor<Gd<TextureRect>>,

    #[export]
    stone_label: OnEditor<Gd<Label>>,

    #[export]
    food_icon: OnEditor<Gd<TextureRect>>,

    #[export]
    food_label: OnEditor<Gd<Label>>,

    #[export]
    gold_icon: OnEditor<Gd<TextureRect>>,

    #[export]
    gold_label: OnEditor<Gd<Label>>,

    #[export]
    game_world: OnEditor<Gd<GameWorld>>,

    cached_amounts: [Option<u32>; ResourceKind::ALL.len()],

    base: Base<HBoxContainer>,
}

#[godot_api]
impl IHBoxContainer for ResourceHeader {
    fn init(base: Base<HBoxContainer>) -> Self {
        Self {
            wood_icon: OnEditor::default(),
            wood_label: OnEditor::default(),
            stone_icon: OnEditor::default(),
            stone_label: OnEditor::default(),
            food_icon: OnEditor::default(),
            food_label: OnEditor::default(),
            gold_icon: OnEditor::default(),
            gold_label: OnEditor::default(),
            game_world: OnEditor::default(),
            cached_amounts: [None; ResourceKind::ALL.len()],
            base,
        }
    }

    fn ready(&mut self) {
        if self.game_world_node().is_none() {
            godot_warn!("ResourceHeader: game_world reference not set");
            return;
        }
        if self.label_nodes().is_none() {
            godot_warn!("ResourceHeader: one or more label references are not set");
            return;
        }
        if self.icon_nodes().is_none() {
            godot_warn!("ResourceHeader: one or more icon references are not set");
            return;
        }

        self.set_icon_textures();

        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        let Some(game_world) = self.game_world_node() else {
            return;
        };
        let Some((mut wood_label, mut stone_label, mut food_label, mut gold_label)) =
            self.label_nodes()
        else {
            return;
        };

        let Some(amounts) = game_world.bind().with_rendered_surface_world(|world| {
            let resources = world.resource::<GameResources>();
            ResourceKind::ALL.map(|kind| resources.get(kind))
        }) else {
            return;
        };

        self.update_label(
            &mut wood_label,
            ResourceKind::Wood,
            amounts[resource_index(ResourceKind::Wood)],
        );
        self.update_label(
            &mut stone_label,
            ResourceKind::Stone,
            amounts[resource_index(ResourceKind::Stone)],
        );
        self.update_label(
            &mut food_label,
            ResourceKind::Food,
            amounts[resource_index(ResourceKind::Food)],
        );
        self.update_label(
            &mut gold_label,
            ResourceKind::Gold,
            amounts[resource_index(ResourceKind::Gold)],
        );
    }
}

impl ResourceHeader {
    fn game_world_node(&self) -> Option<Gd<GameWorld>> {
        let game_world = self.game_world.clone();
        game_world.is_instance_valid().then_some(game_world)
    }

    fn label_nodes(&self) -> Option<ResourceLabels> {
        let wood_label = self.wood_label.clone();
        let stone_label = self.stone_label.clone();
        let food_label = self.food_label.clone();
        let gold_label = self.gold_label.clone();

        (wood_label.is_instance_valid()
            && stone_label.is_instance_valid()
            && food_label.is_instance_valid()
            && gold_label.is_instance_valid())
        .then_some((wood_label, stone_label, food_label, gold_label))
    }

    fn icon_nodes(&self) -> Option<ResourceIcons> {
        let wood_icon = self.wood_icon.clone();
        let stone_icon = self.stone_icon.clone();
        let food_icon = self.food_icon.clone();
        let gold_icon = self.gold_icon.clone();

        (wood_icon.is_instance_valid()
            && stone_icon.is_instance_valid()
            && food_icon.is_instance_valid()
            && gold_icon.is_instance_valid())
        .then_some((wood_icon, stone_icon, food_icon, gold_icon))
    }

    fn set_icon_textures(&self) {
        let Some((mut wood_icon, mut stone_icon, mut food_icon, mut gold_icon)) = self.icon_nodes()
        else {
            return;
        };

        set_icon_texture(&mut wood_icon, ResourceKind::Wood);
        set_icon_texture(&mut stone_icon, ResourceKind::Stone);
        set_icon_texture(&mut food_icon, ResourceKind::Food);
        set_icon_texture(&mut gold_icon, ResourceKind::Gold);
    }

    fn update_label(&mut self, label: &mut Gd<Label>, kind: ResourceKind, amount: u32) {
        let cached_amount = &mut self.cached_amounts[resource_index(kind)];
        if *cached_amount != Some(amount) {
            label.set_text(resource_text(kind, amount).as_str());
            *cached_amount = Some(amount);
        }
    }
}

fn resource_text(kind: ResourceKind, amount: u32) -> String {
    format!("{}: {}", kind.label(), amount)
}

fn resource_index(kind: ResourceKind) -> usize {
    kind as usize
}

fn set_icon_texture(icon: &mut Gd<TextureRect>, kind: ResourceKind) {
    let Some(texture) = load_icon_texture(resource_asset_path(kind)) else {
        return;
    };

    icon.set_expand_mode(texture_rect::ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_texture(&texture);
}

fn load_icon_texture(path: &str) -> Option<Gd<Texture2D>> {
    let Some(mut image) = Image::load_from_file(path) else {
        godot_error!("ResourceHeader: failed to load icon asset {path}");
        return None;
    };

    image.convert(image::Format::RGBA8);
    let Some(texture) = ImageTexture::create_from_image(&image) else {
        godot_error!("ResourceHeader: failed to create icon texture for {path}");
        return None;
    };

    Some(texture.upcast::<Texture2D>())
}

fn resource_asset_path(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => RESOURCE_WOOD_PATH,
        ResourceKind::Stone => RESOURCE_STONE_PATH,
        ResourceKind::Food => RESOURCE_FOOD_PATH,
        ResourceKind::Gold => RESOURCE_GOLD_PATH,
    }
}
