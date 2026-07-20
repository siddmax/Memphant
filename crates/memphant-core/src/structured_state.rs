use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::future::Future;
use std::pin::Pin;

use memphant_types::{
    AdmissionAction, ContextualChunk, EpisodeId, MemoryKind, StoredMemoryUnit, UnitId, UnitState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::validate_valid_interval;

pub const QUANTITY_EVENT_TYPE: &str = "quantity_event.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantityEvent {
    pub namespace: String,
    pub item_key: String,
    pub measure: String,
    pub value: String,
    pub unit: String,
    pub occurred_at: String,
    pub dimensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredStateOperation {
    Create,
    Replace,
    Delete,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredStateOp {
    pub operation: StructuredStateOperation,
    /// Canonical identity resolved by the trusted provider adapter. For
    /// replace/delete, this comes from the targeted active unit, never from
    /// model-authored text.
    pub namespace: String,
    pub item_key: String,
    pub target_unit_ids: Vec<UnitId>,
    pub fields: BTreeMap<String, Value>,
    pub evidence_quote: String,
    /// Exact UTF-8 byte offsets in the parent episode, formatted `start-end`.
    pub source_span: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveStructuredState {
    pub unit_id: UnitId,
    pub namespace: String,
    pub item_key: String,
    pub fields: BTreeMap<String, Value>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredStateProviderIdentity {
    pub model: String,
    pub prompt_hash: String,
    pub schema_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredStateRequest {
    pub episode_id: EpisodeId,
    pub episode_body: String,
    pub active_items: Vec<ActiveStructuredState>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructuredStateProviderError {
    #[error("structured-state provider unavailable: {0}")]
    Unavailable(String),
    #[error("structured-state provider returned invalid output: {0}")]
    InvalidOutput(String),
}

pub trait StructuredStateProvider: Send + Sync {
    fn identity(&self) -> &StructuredStateProviderIdentity;
    fn extract<'a>(
        &'a self,
        request: &'a StructuredStateRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<StructuredStateOp>, StructuredStateProviderError>>
                + Send
                + 'a,
        >,
    >;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedStructuredState {
    pub subject: String,
    pub predicate: String,
    pub body: String,
    pub admission_hint: Option<AdmissionAction>,
    pub contextual_chunks: Vec<ContextualChunk>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    /// `Some([])` is a create precondition, `Some([id, ..])` binds a
    /// replacement/deletion to exact active units, and `None` is an append-only
    /// event. The compiler validates this again at apply time.
    pub target_unit_ids: Option<Vec<UnitId>>,
}

/// Compiler identity is the existing compiler plus a digest over every input
/// that can change model-authored operations. A model, prompt, or schema change
/// therefore cannot hit an older reflect idempotency marker.
pub fn structured_compiler_identity(
    compiler: &str,
    identity: &StructuredStateProviderIdentity,
) -> String {
    let mut hasher = Sha256::new();
    for component in [
        identity.model.as_str(),
        identity.prompt_hash.as_str(),
        identity.schema_hash.as_str(),
    ] {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    let digest = hasher.finalize();
    let suffix: String = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{compiler}+structured-{suffix}")
}

/// Converts provider output into deterministic compiler candidates. Provider
/// text is evidence discovery, never authority. Any invalid operation fails the
/// response closed so model omissions cannot be mistaken for clean extraction.
/// Only an exact quote at the claimed span inside a USER turn can become
/// canonical state.
pub fn project_structured_state(
    episode_id: EpisodeId,
    episode_body: &str,
    operations: &[StructuredStateOp],
) -> Result<Vec<ProjectedStructuredState>, StructuredStateProviderError> {
    let user_ranges = user_turn_ranges(episode_body);
    let mut projected = Vec::new();
    let mut state_identities = BTreeSet::new();
    let mut used_target_ids = HashSet::new();
    for operation in operations {
        let (start, end) = parse_span(&operation.source_span).map_err(|_| {
            StructuredStateProviderError::InvalidOutput(
                "evidence span is not an exact user quote".to_string(),
            )
        })?;
        if start >= end
            || end > episode_body.len()
            || !episode_body.is_char_boundary(start)
            || !episode_body.is_char_boundary(end)
            || episode_body.get(start..end) != Some(operation.evidence_quote.as_str())
            || !user_ranges
                .iter()
                .any(|&(range_start, range_end)| start >= range_start && end <= range_end)
        {
            return Err(StructuredStateProviderError::InvalidOutput(
                "evidence span is not an exact user quote".to_string(),
            ));
        }
        validate_operation_shape(operation)?;
        validate_valid_interval(
            operation.valid_from.as_deref(),
            operation.valid_to.as_deref(),
        )
        .map_err(|error| StructuredStateProviderError::InvalidOutput(error.to_string()))?;

        for target in &operation.target_unit_ids {
            if !used_target_ids.insert(*target) {
                return Err(StructuredStateProviderError::InvalidOutput(
                    "structured-state target is reused in one episode".to_string(),
                ));
            }
        }
        let namespace = canonical_key(&operation.namespace);
        let item_key = canonical_key(&operation.item_key);
        if namespace.is_empty() || item_key.is_empty() {
            return Err(StructuredStateProviderError::InvalidOutput(
                "structured-state identity is not canonicalizable".to_string(),
            ));
        }
        if operation.operation == StructuredStateOperation::Create
            && !state_identities.insert((namespace.clone(), item_key.clone()))
        {
            return Err(StructuredStateProviderError::InvalidOutput(
                "duplicate structured-state identity in one episode".to_string(),
            ));
        }
        if operation.operation != StructuredStateOperation::Append {
            state_identities.insert((namespace.clone(), item_key.clone()));
        }
        if operation.fields.get("type").and_then(Value::as_str) == Some(QUANTITY_EVENT_TYPE) {
            let event = quantity_event_from_fields(&operation.fields).ok_or_else(|| {
                StructuredStateProviderError::InvalidOutput(
                    "quantity fields violate the canonical contract".to_string(),
                )
            })?;
            let grounded_date = crate::parse_content_date(episode_body)
                .map(|date| date.to_string())
                .filter(|date| event.occurred_at.starts_with(date));
            if operation.operation != StructuredStateOperation::Append
                || (grounded_date.is_none()
                    && !operation.evidence_quote.contains(&event.occurred_at))
            {
                return Err(StructuredStateProviderError::InvalidOutput(
                    "quantity occurrence date is not grounded".to_string(),
                ));
            }
        }
        let predicate = match operation.operation {
            StructuredStateOperation::Append => {
                format!("{item_key}@{}:{}-{}", episode_id.as_uuid(), start, end)
            }
            StructuredStateOperation::Create
            | StructuredStateOperation::Replace
            | StructuredStateOperation::Delete => item_key.clone(),
        };
        let (body, admission_hint) = match operation.operation {
            StructuredStateOperation::Delete => (
                format!(
                    "Deleted structured item {namespace}/{} from memory",
                    predicate
                ),
                Some(AdmissionAction::Invalidate),
            ),
            StructuredStateOperation::Create
            | StructuredStateOperation::Replace
            | StructuredStateOperation::Append => (
                format!(
                    "{namespace} item {}: {}",
                    if operation.operation == StructuredStateOperation::Append {
                        item_key.as_str()
                    } else {
                        predicate.as_str()
                    },
                    serde_json::to_string(&operation.fields).map_err(|error| {
                        StructuredStateProviderError::InvalidOutput(error.to_string())
                    })?
                ),
                None,
            ),
        };
        projected.push(ProjectedStructuredState {
            subject: namespace,
            predicate,
            body,
            admission_hint,
            contextual_chunks: vec![ContextualChunk {
                id: format!("evidence-{}-{start}-{end}", episode_id.as_uuid()),
                header: "[structured-state evidence]".to_string(),
                body: operation.evidence_quote.clone(),
                source_span: Some(operation.source_span.clone()),
            }],
            valid_from: operation.valid_from.clone(),
            valid_to: operation.valid_to.clone(),
            target_unit_ids: match operation.operation {
                StructuredStateOperation::Create => Some(Vec::new()),
                StructuredStateOperation::Replace | StructuredStateOperation::Delete => {
                    Some(operation.target_unit_ids.clone())
                }
                StructuredStateOperation::Append => None,
            },
        });
    }
    Ok(projected)
}

fn validate_operation_shape(
    operation: &StructuredStateOp,
) -> Result<(), StructuredStateProviderError> {
    let valid = match operation.operation {
        StructuredStateOperation::Create => {
            operation.target_unit_ids.is_empty() && !operation.fields.is_empty()
        }
        StructuredStateOperation::Replace => {
            !operation.target_unit_ids.is_empty() && !operation.fields.is_empty()
        }
        StructuredStateOperation::Delete => {
            !operation.target_unit_ids.is_empty() && operation.fields.is_empty()
        }
        StructuredStateOperation::Append => {
            operation.target_unit_ids.is_empty() && !operation.fields.is_empty()
        }
    };
    if !valid {
        return Err(StructuredStateProviderError::InvalidOutput(
            "structured-state operation shape is invalid".to_string(),
        ));
    }
    if operation
        .target_unit_ids
        .iter()
        .collect::<HashSet<_>>()
        .len()
        != operation.target_unit_ids.len()
    {
        return Err(StructuredStateProviderError::InvalidOutput(
            "structured-state target ids are duplicated".to_string(),
        ));
    }
    Ok(())
}

/// Returns the provider-visible canonical state for a unit minted by the
/// structured-state projector. Raw episodes and append-only quantity events
/// are deliberately excluded.
pub fn active_structured_state(unit: &StoredMemoryUnit) -> Option<ActiveStructuredState> {
    if unit.state != UnitState::Active
        || unit.kind != MemoryKind::Semantic
        || unit.transaction_to.is_some()
        || unit.contextual_chunks.first()?.header != "[structured-state evidence]"
    {
        return None;
    }
    let (identity, fields_json) = unit.body.split_once(": ")?;
    let (namespace, item_key) = identity.split_once(" item ")?;
    if namespace == QUANTITY_EVENT_TYPE {
        return None;
    }
    let fields = serde_json::from_str::<BTreeMap<String, Value>>(fields_json).ok()?;
    Some(ActiveStructuredState {
        unit_id: unit.id,
        namespace: namespace.to_string(),
        item_key: item_key.to_string(),
        fields,
        valid_from: unit.valid_from.clone(),
        valid_to: unit.valid_to.clone(),
    })
}

const ACTIVE_STATE_SELECTION_THRESHOLD: usize = 32;
const ACTIVE_STATE_SEED_LIMIT: usize = 4;

pub(crate) fn select_relevant_active_state(
    mut items: Vec<ActiveStructuredState>,
    episode_body: &str,
) -> Vec<ActiveStructuredState> {
    items.sort_by_key(|item| item.unit_id.as_uuid());
    if items.len() <= ACTIVE_STATE_SELECTION_THRESHOLD {
        return items;
    }

    let query_tokens = crate::tokenize(episode_body);
    let mut ranked = items
        .iter()
        .map(|item| {
            let fields = serde_json::to_string(&item.fields).unwrap_or_default();
            let body = format!("{} item {}: {fields}", item.namespace, item.item_key);
            (crate::lexical_text_score(&body, &query_tokens), item)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect::<Vec<_>>();
    if ranked.is_empty() {
        return items;
    }
    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .total_cmp(left_score)
            .then_with(|| left.unit_id.as_uuid().cmp(&right.unit_id.as_uuid()))
    });
    let namespaces = ranked
        .into_iter()
        .take(ACTIVE_STATE_SEED_LIMIT)
        .map(|(_, item)| item.namespace.clone())
        .collect::<BTreeSet<_>>();
    items.retain(|item| namespaces.contains(item.namespace.as_str()));
    items
}

pub fn quantity_event_from_body(body: &str) -> Option<QuantityEvent> {
    let (identity, fields) = body.split_once(": ")?;
    let (namespace, item_key) = identity.split_once(" item ")?;
    let fields: BTreeMap<String, Value> = serde_json::from_str(fields).ok()?;
    let mut event = quantity_event_from_fields(&fields)?;
    event.namespace = namespace.to_string();
    event.item_key = item_key.to_string();
    Some(event)
}

/// Parses the canonical quantity-event field contract shared by provider
/// adapters and the projection/recall path.
pub fn quantity_event_from_fields(fields: &BTreeMap<String, Value>) -> Option<QuantityEvent> {
    const KEYS: [&str; 6] = [
        "dimensions",
        "measure",
        "occurred_at",
        "type",
        "unit",
        "value",
    ];
    if fields.len() != KEYS.len() || !KEYS.iter().all(|key| fields.contains_key(*key)) {
        return None;
    }
    if fields.get("type")?.as_str()? != QUANTITY_EVENT_TYPE {
        return None;
    }
    let measure = fields.get("measure")?.as_str()?.to_string();
    let unit = fields.get("unit")?.as_str()?.to_string();
    if measure.is_empty()
        || unit.is_empty()
        || canonical_key(&measure) != measure
        || canonical_key(&unit) != unit
    {
        return None;
    }
    let value = fields.get("value")?.as_str()?.to_string();
    if !valid_decimal(&value) {
        return None;
    }
    let occurred_at = fields.get("occurred_at")?.as_str()?.to_string();
    occurred_at.parse::<jiff::Timestamp>().ok()?;
    let dimensions = fields
        .get("dimensions")?
        .as_object()?
        .iter()
        .map(|(key, value)| {
            (!key.is_empty()
                && canonical_key(key) == *key
                && matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)))
            .then(|| (key.clone(), value.clone()))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    Some(QuantityEvent {
        namespace: String::new(),
        item_key: String::new(),
        measure,
        value,
        unit,
        occurred_at,
        dimensions,
    })
}

fn valid_decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    if value.is_empty() {
        return false;
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.len() <= 18
        && (fraction.is_empty() || fraction.bytes().all(|byte| byte.is_ascii_digit()))
        && !value.ends_with('.')
}

fn parse_span(span: &str) -> Result<(usize, usize), ()> {
    let (start, end) = span.split_once('-').ok_or(())?;
    Ok((start.parse().map_err(|_| ())?, end.parse().map_err(|_| ())?))
}

fn canonical_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Resolves an exact evidence quote to its unique UTF-8 byte span inside a
/// user-authored turn. Models identify evidence; deterministic code owns byte
/// arithmetic and rejects ambiguous or non-user matches.
pub fn ground_user_evidence_quote(body: &str, quote: &str) -> Option<String> {
    if quote.is_empty() {
        return None;
    }
    let user_ranges = user_turn_ranges(body);
    let mut matches = body.match_indices(quote).filter_map(|(start, value)| {
        let end = start + value.len();
        user_ranges
            .iter()
            .any(|&(range_start, range_end)| start >= range_start && end <= range_end)
            .then_some((start, end))
    });
    let (start, end) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(format!("{start}-{end}"))
}

/// Returns exact user-authored turn contents suitable for a constrained
/// evidence-quote schema. Role prefixes and separator newlines are excluded.
pub fn user_evidence_turns(body: &str) -> Vec<String> {
    user_turn_ranges(body)
        .into_iter()
        .filter_map(|(start, end)| body.get(start..end))
        .map(|turn| turn.trim_end().to_string())
        .filter(|turn| !turn.is_empty())
        .fold(Vec::new(), |mut turns, turn| {
            if !turns.contains(&turn) {
                turns.push(turn);
            }
            turns
        })
}

fn user_turn_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut user_start = None;
    let mut offset = 0;
    for inclusive_line in body.split_inclusive('\n') {
        let line = inclusive_line.strip_suffix('\n').unwrap_or(inclusive_line);
        let trimmed_start = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if let Some((role, colon)) = role_prefix(trimmed) {
            if let Some(start) = user_start.take()
                && start < offset
            {
                ranges.push((start, offset));
            }
            if matches!(role, "user" | "user_agent") {
                let content = &trimmed[colon + 1..];
                let leading = content.len() - content.trim_start().len();
                let start = offset + trimmed_start + colon + 1 + leading;
                user_start = Some(start);
            }
        }
        offset += inclusive_line.len();
    }
    if let Some(start) = user_start
        && start < body.len()
    {
        ranges.push((start, body.len()));
    }
    ranges
}

fn role_prefix(line: &str) -> Option<(&str, usize)> {
    let (role, content) = line.split_once(':')?;
    if role.is_empty()
        || !role.as_bytes()[0].is_ascii_lowercase()
        || !role
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || (!content.is_empty() && !content.as_bytes()[0].is_ascii_whitespace())
    {
        return None;
    }
    Some((role, role.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u128, namespace: &str, item_key: &str, value: &str) -> ActiveStructuredState {
        ActiveStructuredState {
            unit_id: UnitId::from_u128(id),
            namespace: namespace.to_string(),
            item_key: item_key.to_string(),
            fields: BTreeMap::from([("value".to_string(), Value::String(value.to_string()))]),
            valid_from: None,
            valid_to: None,
        }
    }

    fn large_state() -> Vec<ActiveStructuredState> {
        let mut items = vec![
            item(
                1,
                "accessibility_review",
                "action_item",
                "Yuki Tanaka spatial audio refactor",
            ),
            item(2, "accessibility_review", "title", "VR navigation module"),
        ];
        items.extend((3..=33).map(|id| {
            item(
                id,
                &format!("unrelated_{id}"),
                "detail",
                &format!("noise_{id}"),
            )
        }));
        items
    }

    #[test]
    fn large_state_selects_exact_target_and_its_namespace_siblings() {
        let selected = select_relevant_active_state(
            large_state(),
            "user_agent: Remove Yuki Tanaka's spatial audio refactor.\n\
             ai_agent: Understood. Is there anything else?\n",
        );
        let ids = selected.iter().map(|item| item.unit_id).collect::<Vec<_>>();

        assert!(ids.contains(&UnitId::from_u128(1)));
        assert!(ids.contains(&UnitId::from_u128(2)));
        assert!(selected.len() < 33);
    }

    #[test]
    fn small_state_passes_through_in_deterministic_unit_order() {
        let selected = select_relevant_active_state(
            vec![
                item(2, "todos", "second", "beta"),
                item(1, "todos", "first", "alpha"),
            ],
            "user: alpha",
        );

        assert_eq!(
            selected.iter().map(|item| item.unit_id).collect::<Vec<_>>(),
            vec![UnitId::from_u128(1), UnitId::from_u128(2)]
        );
    }

    #[test]
    fn large_state_selection_is_deterministic_across_input_order() {
        let items = large_state();
        let mut reversed = items.clone();
        reversed.reverse();
        let episode = "user: Remove Yuki Tanaka's spatial audio refactor.";

        assert_eq!(
            select_relevant_active_state(items, episode),
            select_relevant_active_state(reversed, episode)
        );
    }

    #[test]
    fn no_meaningful_user_signal_falls_back_to_all_state() {
        let selected = select_relevant_active_state(large_state(), "user: zyzzyva quokka");

        assert_eq!(selected.len(), 33);
    }

    #[test]
    fn assistant_target_context_resolves_anaphoric_user_mutation() {
        let mut items = (1..=33)
            .map(|id| {
                item(
                    id,
                    &format!("unrelated_{id}"),
                    "detail",
                    &format!("noise_{id}"),
                )
            })
            .collect::<Vec<_>>();
        items.push(item(
            100,
            "accessibility_review",
            "action",
            "Yuki Tanaka spatial audio refactor",
        ));
        items.push(item(
            101,
            "accessibility_review",
            "title",
            "VR navigation module",
        ));
        let selected = select_relevant_active_state(
            items,
            "ai_agent: Do you mean the Yuki Tanaka spatial audio refactor?\n\
             user_agent: Yes, remove that item.\n",
        );
        assert!(
            selected
                .iter()
                .any(|item| item.unit_id == UnitId::from_u128(100))
        );
    }
}
