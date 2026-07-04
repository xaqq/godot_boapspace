use game_engine::resources::{GameResources, ResourceKind};

#[test]
fn test_multiple_independent_resources() {
    let mut r = GameResources::default();
    assert!(r.add(ResourceKind::Wood, 100));
    assert!(r.add(ResourceKind::Stone, 200));
    assert_eq!(r.get(ResourceKind::Wood), 100);
    assert_eq!(r.get(ResourceKind::Stone), 200);
    assert!(r.remove(ResourceKind::Wood, 50));
    assert_eq!(r.get(ResourceKind::Wood), 50);
    assert_eq!(r.get(ResourceKind::Stone), 200);
}

#[test]
fn test_add_does_not_overflow_other_fields() {
    let mut r = GameResources::default();
    assert!(r.add(ResourceKind::Gold, 999));
    assert_eq!(r.get(ResourceKind::Gold), 999);
    assert_eq!(r.get(ResourceKind::Wood), 0);
    assert_eq!(r.get(ResourceKind::Stone), 0);
    assert_eq!(r.get(ResourceKind::Food), 0);
}

#[test]
fn test_remove_returns_false_on_insufficient() {
    let mut r = GameResources::new(5, 0, 0, 0);
    assert!(!r.remove(ResourceKind::Wood, 10));
    assert_eq!(r.get(ResourceKind::Wood), 5);
}

#[test]
fn test_add_returns_false_on_overflow() {
    let mut r = GameResources::new(0, 0, 0, u32::MAX);
    assert!(!r.add(ResourceKind::Gold, 1));
    assert_eq!(r.get(ResourceKind::Gold), u32::MAX);
}

#[test]
fn test_multiple_add_remove_sequence() {
    let mut r = GameResources::default();
    assert!(r.add(ResourceKind::Food, 10));
    assert!(r.remove(ResourceKind::Food, 3));
    assert_eq!(r.get(ResourceKind::Food), 7);
    assert!(r.add(ResourceKind::Food, 5));
    assert_eq!(r.get(ResourceKind::Food), 12);
    assert!(r.remove(ResourceKind::Food, 12));
    assert_eq!(r.get(ResourceKind::Food), 0);
    assert!(!r.remove(ResourceKind::Food, 1));
}
