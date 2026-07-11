use game_engine::resources::{ResourceAmounts, ResourceKind};

#[test]
fn test_multiple_independent_resource_amounts() {
    let mut amounts = ResourceAmounts::zero();
    amounts.set(ResourceKind::Wood, 100);
    amounts.set(ResourceKind::Stone, 200);

    assert_eq!(amounts.get(ResourceKind::Wood), 100);
    assert_eq!(amounts.get(ResourceKind::Stone), 200);
    assert_eq!(amounts.get(ResourceKind::Food), 0);
    assert_eq!(amounts.get(ResourceKind::Gold), 0);
}

#[test]
fn test_resource_amounts_default_to_zero() {
    let amounts = ResourceAmounts::default();

    for kind in ResourceKind::ALL {
        assert_eq!(amounts.get(kind), 0);
    }
}

#[test]
fn test_resource_amounts_get_matches_constructor() {
    let amounts = ResourceAmounts::of(ResourceKind::Wood, 1)
        .with(ResourceKind::Stone, 2)
        .with(ResourceKind::Food, 3)
        .with(ResourceKind::Gold, 4)
        .with(ResourceKind::Crops, 5)
        .with(ResourceKind::WildBerries, 6)
        .with(ResourceKind::Planks, 7)
        .with(ResourceKind::StoneBlocks, 8);

    assert_eq!(amounts.get(ResourceKind::Wood), 1);
    assert_eq!(amounts.get(ResourceKind::Stone), 2);
    assert_eq!(amounts.get(ResourceKind::Food), 3);
    assert_eq!(amounts.get(ResourceKind::Gold), 4);
    assert_eq!(amounts.get(ResourceKind::Crops), 5);
    assert_eq!(amounts.get(ResourceKind::WildBerries), 6);
    assert_eq!(amounts.get(ResourceKind::Planks), 7);
    assert_eq!(amounts.get(ResourceKind::StoneBlocks), 8);
    assert_eq!(amounts.total(), 36);
}

#[test]
fn test_resource_kind_discriminants_and_iteration_contracts_are_stable() {
    assert_eq!(
        ResourceKind::ALL.map(|kind| kind as i64),
        [0, 1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(
        ResourceKind::NATURAL,
        [
            ResourceKind::Wood,
            ResourceKind::Stone,
            ResourceKind::WildBerries,
            ResourceKind::Gold,
        ]
    );
}

#[test]
fn test_resource_kind_labels_are_present() {
    for kind in ResourceKind::ALL {
        assert!(!kind.label().is_empty());
    }
}

#[test]
fn test_resource_kind_descriptions_are_present() {
    for kind in ResourceKind::ALL {
        assert!(!kind.description().is_empty());
    }
}
