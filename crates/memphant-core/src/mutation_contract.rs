use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{MutationVerb, StoreError};

const HASH_DOMAIN: &[u8] = b"memphant-mutation-v1\0";

pub fn validate_idempotency_key(key: &str) -> Result<(), StoreError> {
    if key.trim().is_empty() || key.len() > 255 {
        return Err(StoreError::Conflict(
            "idempotency key must contain 1 to 255 bytes".to_string(),
        ));
    }
    Ok(())
}

pub fn canonical_mutation_request_hash<T: Serialize>(
    verb: MutationVerb,
    request: &T,
) -> Result<[u8; 32], StoreError> {
    let canonical = canonicalize(
        serde_json::to_value(request)
            .map_err(|error| StoreError::Conflict(format!("invalid mutation request: {error}")))?,
    );
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| StoreError::Conflict(format!("invalid mutation request: {error}")))?;
    let mut hasher = Sha256::new();
    hasher.update(HASH_DOMAIN);
    hasher.update(verb.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(bytes);
    Ok(hasher.finalize().into())
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect(),
            )
        }
        scalar => scalar,
    }
}
