use jrg::query_expansion::expand;

#[test]
fn numeric_boundaries_keep_inclusive_and_strict_meanings_separate() {
    for (query, required, forbidden) in [
        ("zero or negative", "non-positive", "nonnegative"),
        ("zero or positive", "non-negative", "nonpositive"),
        ("greater than zero", "strictly positive", "nonnegative"),
        ("less than zero", "strictly negative", "nonpositive"),
    ] {
        let result = expand(query).unwrap();
        assert!(result.query.starts_with(query));
        assert!(result.added_phrases.iter().any(|p| p == required));
        assert!(!result.applied_rule_ids.iter().any(|p| p == forbidden));
    }
}

#[test]
fn scoped_official_terms_and_complete_phrases_avoid_unrelated_inputs() {
    let result = expand("S3 regular buckets and directory buckets").unwrap();
    assert!(
        result
            .added_phrases
            .iter()
            .any(|p| p == "general purpose buckets")
    );
    for unrelated in [
        "regular buckets in a garden",
        "zerox or negatives",
        "nonpositiveIdentifier",
        "not non-negative",
        "not zero or negative",
    ] {
        assert!(
            expand(unrelated).unwrap().added_phrases.is_empty(),
            "{unrelated}"
        );
    }
}

#[test]
fn common_terms_are_deterministic_bounded_and_do_not_chain() {
    let query = "unbounded rate limits with retries and exponentially increasing delay";
    let a = expand(query).unwrap();
    assert_eq!(a, expand(query).unwrap());
    assert!(!a.rule_version.is_empty());
    assert!(a.added_phrases.iter().any(|p| p == "unlimited"));
    assert!(a.added_phrases.len() <= 4);
    assert!(a.query.len() - query.len() <= 512);
    assert!(a.query.starts_with(query));
    let long = format!(
        "zero or negative{}",
        " ".repeat(4096 - "zero or negative".len())
    );
    assert_eq!(expand(&long).unwrap().query, long);
}

#[test]
fn text_without_a_rule_and_existing_aliases_are_not_rewritten() {
    assert_eq!(
        expand("キャンセル foo_bar v1.2").unwrap().query,
        "キャンセル foo_bar v1.2"
    );
    let result =
        expand("zero or negative non-positive at most zero less than or equal to zero").unwrap();
    assert!(result.added_phrases.is_empty());
}
