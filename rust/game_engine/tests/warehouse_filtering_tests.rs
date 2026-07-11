use bevy_ecs::prelude::World;
use game_engine::buildings::WarehouseInventory;
use game_engine::components::{CarriedResource, FoodPouch};
use game_engine::resources::{resource_overview, ResourceKind};

#[test]
fn warehouse_filter_defaults_to_all_allowed_and_accepts_allow_none() {
    let mut inventory = WarehouseInventory::empty();

    for kind in ResourceKind::ALL {
        assert!(inventory.is_allowed(kind));
        inventory.set_allowed(kind, false);
        assert!(!inventory.is_allowed(kind));
    }

    for kind in ResourceKind::ALL {
        assert!(!inventory.add(kind, 1));
    }
    assert_eq!(inventory.used_size(), 0);
}

#[test]
fn disallowing_a_resource_blocks_future_deposits_but_preserves_withdrawals() {
    let mut inventory = WarehouseInventory::empty();
    assert!(inventory.add(ResourceKind::Wood, 4));

    inventory.set_allowed(ResourceKind::Wood, false);

    assert_eq!(inventory.contents().get(ResourceKind::Wood), 4);
    assert!(!inventory.add(ResourceKind::Wood, 1));
    assert!(inventory.consume(ResourceKind::Wood, 3));
    assert_eq!(inventory.contents().get(ResourceKind::Wood), 1);
}

#[test]
fn carried_resource_enforces_one_kind_five_unit_atomic_capacity() {
    let mut cargo = CarriedResource::empty();
    assert!(cargo.add(ResourceKind::Wood, 3));
    assert!(!cargo.add(ResourceKind::Stone, 1));
    assert!(!cargo.add(ResourceKind::Wood, 3));
    assert_eq!(cargo.contents().get(ResourceKind::Wood), 3);

    assert!(cargo.add(ResourceKind::Wood, 2));
    assert_eq!(cargo.used_size(), 5);
    assert!(cargo.consume(ResourceKind::Wood, 5));
    assert_eq!(cargo.stack(), None);
    assert!(cargo.add(ResourceKind::Stone, 5));
}

#[test]
fn food_pouch_and_cargo_are_independent_and_only_cargo_is_colony_stock() {
    let mut world = World::new();
    world.spawn((
        FoodPouch::new(100),
        CarriedResource::of(ResourceKind::Food, 5),
    ));

    let overview = resource_overview(&mut world);

    assert_eq!(overview.usable().get(ResourceKind::Food), 5);
}

#[test]
fn food_pouch_rejects_overfill_atomically() {
    let mut pouch = FoodPouch::new(99);

    assert!(!pouch.add(2));
    assert_eq!(pouch.amount(), 99);
    assert!(pouch.add(1));
    assert_eq!(pouch.amount(), 100);
}
