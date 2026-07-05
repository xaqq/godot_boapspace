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
    let amounts = ResourceAmounts::new(1, 2, 3, 4);

    assert_eq!(amounts.get(ResourceKind::Wood), 1);
    assert_eq!(amounts.get(ResourceKind::Stone), 2);
    assert_eq!(amounts.get(ResourceKind::Food), 3);
    assert_eq!(amounts.get(ResourceKind::Gold), 4);
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
