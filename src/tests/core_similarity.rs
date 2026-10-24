//! Unit tests for `src/core/similarity.rs`, loaded there as its `tests` module.

use super::*;

#[test]
fn test_similarity_algorithm() {
    let mut set1 = HashSet::new();
    set1.insert("thesis about intelligence".to_string());
    set1.insert("applied artificial intelligence".to_string());
    set1.insert("experiment results".to_string());

    let mut set2 = HashSet::new();
    set2.insert("thesis about intelligence".to_string());
    set2.insert("applied artificial intelligence".to_string());
    set2.insert("another different section".to_string());

    let (sim, count) = calculate_jaccard_similarity(&set1, &set2);
    assert_eq!(count, 2);
    assert!(sim >= 50.0, "Should have at least 50% similarity");
}
