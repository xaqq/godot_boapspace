use game_engine::resources::{GameResources, ResourceKind};

#[test]
fn test_multiple_independent_resources() {
    let mut r = GameResources::default();
    r.add(ResourceKind::Wood, 100);
    r.add(ResourceKind::Stone, 200);
    assert_eq!(r.wood, 100);
    assert_eq!(r.stone, 200);
    r.remove(ResourceKind::Wood, 50);
    assert_eq!(r.wood, 50);
    assert_eq!(r.stone, 200);
}

#[test]
fn test_add_does_not_overflow_other_fields() {
    let mut r = GameResources::default();
    r.add(ResourceKind::Gold, 999);
    assert_eq!(r.gold, 999);
    assert_eq!(r.wood, 0);
    assert_eq!(r.stone, 0);
    assert_eq!(r.food, 0);
}

#[test]
fn test_remove_returns_false_on_insufficient() {
    let mut r = GameResources {
        wood: 5,
        ..Default::default()
    };
    assert!(!r.remove(ResourceKind::Wood, 10));
    assert_eq!(r.wood, 5);
}

#[test]
fn test_multiple_add_remove_sequence() {
    let mut r = GameResources::default();
    r.add(ResourceKind::Food, 10);
    assert!(r.remove(ResourceKind::Food, 3));
    assert_eq!(r.food, 7);
    r.add(ResourceKind::Food, 5);
    assert_eq!(r.food, 12);
    assert!(r.remove(ResourceKind::Food, 12));
    assert_eq!(r.food, 0);
    assert!(!r.remove(ResourceKind::Food, 1));
}
