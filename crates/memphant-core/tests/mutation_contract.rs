use memphant_core::{MutationVerb, canonical_mutation_request_hash, validate_idempotency_key};
use serde_json::Value;

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn idempotency_keys_are_nonblank_and_at_most_255_bytes() {
    assert!(validate_idempotency_key("a").is_ok());
    assert!(validate_idempotency_key(&"a".repeat(255)).is_ok());
    assert!(validate_idempotency_key("").is_err());
    assert!(validate_idempotency_key(" \t").is_err());
    assert!(validate_idempotency_key(&"a".repeat(256)).is_err());
}

#[test]
fn mutation_hash_is_canonical_domain_separated_and_pinned() {
    let compact: Value = serde_json::from_str(r#"{"z":1,"a":{"y":2,"x":3}}"#).unwrap();
    let reordered: Value = serde_json::from_str(
        r#"
        {
          "a": { "x": 3, "y": 2 },
          "z": 1
        }
        "#,
    )
    .unwrap();

    let retained = canonical_mutation_request_hash(MutationVerb::Retain, &compact).unwrap();
    assert_eq!(
        hex(retained),
        "4dc7caf914351828ad5e768b61cf40d1388e3975ad07b5faef9cd0e4cb83f2df"
    );
    assert_eq!(
        retained,
        canonical_mutation_request_hash(MutationVerb::Retain, &reordered).unwrap()
    );

    let changed: Value = serde_json::from_str(r#"{"a":{"x":3,"y":2},"z":2}"#).unwrap();
    assert_ne!(
        retained,
        canonical_mutation_request_hash(MutationVerb::Retain, &changed).unwrap()
    );
    assert_ne!(
        retained,
        canonical_mutation_request_hash(MutationVerb::Correct, &compact).unwrap()
    );
}
