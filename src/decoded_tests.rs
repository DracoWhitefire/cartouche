use super::*;

#[test]
fn debug_impl_includes_value_and_warnings() {
    let mut d: Decoded<u32, &str> = Decoded::new(42u32);
    d.push_warning("something odd");
    let s = alloc::format!("{d:?}");
    assert!(s.contains("42"));
    assert!(s.contains("something odd"));
}

#[test]
fn debug_impl_empty_warnings() {
    let d: Decoded<u32, &str> = Decoded::new(7u32);
    let s = alloc::format!("{d:?}");
    assert!(s.contains("7"));
}
