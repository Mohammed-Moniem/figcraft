#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn pinned_value_layout_fits_the_documented_structure_allowance() {
    assert!(std::mem::size_of::<serde_json::Value>() <= 64);
    assert!(std::mem::size_of::<String>() <= 24);
    assert_eq!(STRUCTURE_BYTES, 4096);
}

#[test]
fn compact_structure_is_refused_before_materialization_while_larger_string_fits() {
    let budget = Arc::new(Budget::new(64 * 1024));
    let string = format!("\"{}\"", "x".repeat(2048));
    let string_owner = budget.parsed(string.as_bytes()).expect("one long string fits");
    assert!(budget.used() > string.len(), "wire bytes alone are not the accounting unit");
    drop(string_owner);
    let dense = format!("[{}]", std::iter::repeat_n("null", 128).collect::<Vec<_>>().join(","));
    assert!(dense.len() < string.len());
    assert!(budget.parsed(dense.as_bytes()).is_none(), "smaller dense JSON must pay for its expanded tree before serde");
    assert_eq!(budget.parses.load(Ordering::Relaxed), 0, "preflight does not materialize either input");
    assert_eq!(budget.used(), 0);
}

#[test]
fn escaped_structural_characters_are_string_content_not_tree_units() {
    let budget = Arc::new(Budget::new(12 * 1024));
    let escaped = br#""\"{}[],:null\\\"{}[],:false""#;
    let owner = budget.parsed(escaped).expect("escaped quotes and delimiters stay within one string");
    assert!(serde_json::from_slice::<serde_json::Value>(escaped).is_ok(), "fixture is valid escaped JSON");
    drop(owner);
    assert_eq!(budget.used(), 0);
}

#[test]
fn duplicate_and_unknown_fields_pay_before_serde_can_collapse_them() {
    let budget = Arc::new(Budget::new(64 * 1024));
    let duplicate = format!("{{{}}}", std::iter::repeat_n("\"unknown\":0", 64).collect::<Vec<_>>().join(","));
    assert_eq!(serde_json::from_str::<serde_json::Value>(&duplicate).unwrap().as_object().unwrap().len(), 1);
    assert!(budget.parsed(duplicate.as_bytes()).is_none(), "all duplicate occurrences are charged before materialization");
    assert_eq!(budget.used(), 0);
}

#[test]
fn excessive_nesting_is_rejected_without_allocating_a_parse_tree() {
    let budget = Arc::new(Budget::new(2 * 1024 * 1024));
    let accepted = format!("{}{}", "[".repeat(128), "]".repeat(128));
    let owner = budget.parsed(accepted.as_bytes()).expect("preflight allows its documented maximum nesting");
    drop(owner);
    let refused = format!("[{}]", accepted);
    assert!(budget.parsed(refused.as_bytes()).is_none());
    assert_eq!(budget.used(), 0);
    assert_eq!(budget.parses.load(Ordering::Relaxed), 0);
}

#[test]
fn aggregate_reservations_release_only_after_the_last_real_owner() {
    let budget = Arc::new(Budget::new(24 * 1024));
    let first = budget.parsed(b"null").unwrap();
    let first_copy = first.clone();
    let second = budget.parsed(b"null").unwrap();
    let held = budget.used();
    assert!(budget.parsed(b"null").is_none(), "one shared pool limits simultaneous independent owners");
    drop(first);
    assert_eq!(budget.used(), held, "dropping one Arc cannot free a live payload");
    drop(first_copy);
    let recovered = budget.parsed(b"null").expect("last-owner release immediately restores admission");
    drop((second, recovered));
    assert_eq!(budget.used(), 0);
}

#[test]
fn failed_admission_does_not_consume_existing_owners_or_future_capacity() {
    let budget = Arc::new(Budget::new(16 * 1024));
    let held = budget.parsed(b"null").unwrap();
    let before = budget.used();
    assert!(budget.parsed(b"[0,0,0,0]").is_none());
    assert_eq!(budget.used(), before);
    drop(held);
    let recovered = budget.parsed(b"null").unwrap();
    drop(recovered);
    assert_eq!(budget.used(), 0);
}

#[test]
fn shared_budget_identity_survives_independent_handle_lookups() {
    assert!(Arc::ptr_eq(&Budget::shared(), &Budget::shared()), "rebound listeners must obtain the same process-wide pool");
}
