//! W6 deterministic fact extraction, end-to-end through the service
//! (retain → reflect): when `with_fact_extraction_enabled` is on, the reflect
//! stage mines first-person preference/attribute statements from an episode's
//! USER turns and emits EXTRA short, honest-subject-key ReflectCandidates that
//! flow through the existing admission machinery — so the "my favorite X is now
//! Z" update chain finally supersedes. All behind the flag, default off, so the
//! flag-off path is byte-identical to today.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{FixedClock, InMemoryStore, NoopEmbedding, derive_subject_key};
use memphant_types::{ActorId, RetainEpisodeHttpRequest, ScopeId, TenantId, TrustLevel, UnitState};

const CLOCK: FixedClock = FixedClock("2026-07-10T00:00:00Z");

fn service(store: InMemoryStore, fact_extraction: bool) -> MemoryService<InMemoryStore> {
    MemoryService::new(Arc::new(store), Arc::new(CLOCK), Arc::new(NoopEmbedding))
        .with_fact_extraction_enabled(fact_extraction)
}

fn retain_request(
    tenant: TenantId,
    scope: ScopeId,
    actor: ActorId,
    body: &str,
) -> RetainEpisodeHttpRequest {
    RetainEpisodeHttpRequest {
        tenant_id: tenant,
        scope_id: scope,
        actor_id: actor,
        source_kind: "user".to_string(),
        source_trust: TrustLevel::TrustedUser,
        subject_hint: None,
        subject: None,
        predicate: None,
        body: Some(body.to_string()),
        resource: None,
        unit: None,
        compiler_version: None,
    }
}

async fn retain_and_reflect(
    svc: &MemoryService<InMemoryStore>,
    tenant: TenantId,
    scope: ScopeId,
    actor: ActorId,
    body: &str,
) {
    svc.retain(tenant, retain_request(tenant, scope, actor, body))
        .await
        .expect("retain");
    svc.reflect(tenant, scope, None).await.expect("reflect");
}

/// §5 update-chain integration (the headline test): episode A asserts
/// "my favorite tea is chamomile"; a LATER episode B asserts "my favorite tea is
/// rooibos now". Both mine the SAME honest subject key
/// (`{scope}:preference:favorite tea`), so B's fact unit SUPERSEDES A's through
/// the existing subject-key machinery — the update chain fires without any new
/// admission code.
#[tokio::test]
async fn favorite_update_across_episodes_supersedes_prior_fact() {
    let store = InMemoryStore::default();
    let svc = service(store.clone(), true);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s1]\n\
user: My favorite tea is chamomile.\n\
assistant: Chamomile is very calming.\n\
user: I drink it every single evening.\n",
    )
    .await;
    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s2]\n\
user: My favorite tea is rooibos now.\n\
assistant: Rooibos is a lovely switch.\n\
user: I changed it just last week.\n",
    )
    .await;

    let fact_key = derive_subject_key(
        scope.as_uuid(),
        Some("preference"),
        Some("favorite tea"),
        "",
    );
    let units = store.memory_units(tenant);
    let fact_units: Vec<_> = units
        .iter()
        .filter(|unit| unit.subject_key.as_deref() == Some(fact_key.as_str()))
        .collect();
    assert_eq!(
        fact_units.len(),
        2,
        "two generations of the favorite-tea fact: {fact_units:?}"
    );

    let active: Vec<_> = fact_units
        .iter()
        .filter(|unit| unit.state == UnitState::Active)
        .collect();
    let superseded: Vec<_> = fact_units
        .iter()
        .filter(|unit| unit.state == UnitState::Superseded)
        .collect();
    assert_eq!(active.len(), 1, "exactly one active generation wins");
    assert!(
        active[0].body.contains("rooibos"),
        "the later fact wins: {}",
        active[0].body
    );
    assert_eq!(superseded.len(), 1, "exactly one prior generation retired");
    assert!(
        superseded[0].body.contains("chamomile"),
        "the earlier fact is superseded: {}",
        superseded[0].body
    );

    // The supersedence machinery also wrote the Supersedes/Contradicts edges.
    let edges = store.memory_edges(tenant);
    assert!(
        edges.iter().any(
            |edge| edge.kind == memphant_types::MemoryEdgeKind::Supersedes
                && edge.src_id == active[0].id
                && edge.dst_id == superseded[0].id
        ),
        "a Supersedes edge points from the winner to the retired generation"
    );
}

/// §2 citation + shape: the extracted fact unit is SHORT (the verbatim
/// sentence, not the whole session), is cited back to the parent episode, and
/// carries NO contextual chunks (§3).
#[tokio::test]
async fn extracted_fact_unit_is_short_cited_and_chunkless() {
    let store = InMemoryStore::default();
    let svc = service(store.clone(), true);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    let episode = store_episode_id(
        &svc,
        &store,
        tenant,
        scope,
        actor,
        "[session s1]\n\
user: My name is Sidney Carter.\n\
assistant: Nice to meet you Sidney.\n\
user: I work in downtown Boston.\n",
    )
    .await;

    let name_key = derive_subject_key(scope.as_uuid(), Some("attribute"), Some("name"), "");
    let units = store.memory_units(tenant);
    let fact = units
        .iter()
        .find(|unit| unit.subject_key.as_deref() == Some(name_key.as_str()))
        .expect("the name fact was mined");
    assert_eq!(
        fact.body, "My name is Sidney Carter",
        "body is the verbatim sentence, not the whole session"
    );
    assert_eq!(
        fact.source_episode_id,
        Some(episode),
        "the fact cites its parent episode"
    );
    assert!(
        fact.contextual_chunks.is_empty(),
        "extracted facts never carry contextual chunks"
    );
}

async fn store_episode_id(
    svc: &MemoryService<InMemoryStore>,
    store: &InMemoryStore,
    tenant: TenantId,
    scope: ScopeId,
    actor: ActorId,
    body: &str,
) -> memphant_types::EpisodeId {
    retain_and_reflect(svc, tenant, scope, actor, body).await;
    store.episodes(tenant).last().expect("episode stored").id
}

/// §3 assistant-turn exclusion: a first-person statement inside an ASSISTANT
/// turn is never mined; only the user turn's fact is extracted.
#[tokio::test]
async fn assistant_turns_are_never_mined() {
    let store = InMemoryStore::default();
    let svc = service(store.clone(), true);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s1]\n\
assistant: I love that idea and my favorite part is the ending.\n\
user: My favorite color is deep blue.\n\
assistant: I really like blue too, it is calming.\n",
    )
    .await;

    let units = store.memory_units(tenant);
    let fact_units: Vec<_> = units
        .iter()
        .filter(|unit| {
            unit.subject_key
                .as_deref()
                .is_some_and(|key| key.contains(":preference:") || key.contains(":attribute:"))
        })
        .collect();
    assert_eq!(
        fact_units.len(),
        1,
        "only the user turn is mined: {:?}",
        fact_units
            .iter()
            .map(|unit| unit.body.clone())
            .collect::<Vec<_>>()
    );
    assert!(fact_units[0].body.contains("deep blue"));
}

/// §5 flag-off byte-identical reflect: with fact extraction OFF, only the raw
/// episode unit is compiled — no extra fact units, exactly as before W6.
#[tokio::test]
async fn flag_off_emits_no_fact_units() {
    let store = InMemoryStore::default();
    let svc = service(store.clone(), false);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s1]\n\
user: My favorite tea is chamomile.\n\
assistant: Chamomile is very calming.\n\
user: I really love hiking in the hills.\n",
    )
    .await;

    let units = store.memory_units(tenant);
    assert_eq!(
        units.len(),
        1,
        "flag off ⇒ only the raw episode unit, no fact units: {:?}",
        units
            .iter()
            .map(|unit| unit.body.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        units[0].body.contains("[session s1]"),
        "the single unit is the raw episode body"
    );
}

/// §3 caps + within-episode dedup: an episode packed with distinct facts is
/// capped at 8, and a subject asserted twice in the same episode keeps only the
/// LAST value (later turns win).
#[tokio::test]
async fn caps_and_within_episode_dedup() {
    let store = InMemoryStore::default();
    let svc = service(store.clone(), true);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    // 10 distinct preference facts (cap is 8) plus a favorite-tea asserted twice
    // (green tea, then oolong): the second assertion must win.
    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s1]\n\
user: I really love mountain hiking trips.\n\
user: I really enjoy long distance cycling.\n\
user: I really like ocean kayaking a lot.\n\
user: I really adore quiet forest camping.\n\
user: I really prefer strong black coffee.\n\
user: I really love spicy thai cooking.\n\
user: I really enjoy classic jazz records.\n\
user: I really like vintage film cameras.\n\
user: I really love slow sunday mornings.\n\
user: I really enjoy fresh mountain air.\n\
user: My favorite tea is plain green tea.\n\
user: My favorite tea is smoky oolong tea.\n",
    )
    .await;

    let units = store.memory_units(tenant);
    let fact_units: Vec<_> = units
        .iter()
        .filter(|unit| {
            unit.subject_key
                .as_deref()
                .is_some_and(|key| key.contains(":preference:") || key.contains(":attribute:"))
        })
        .collect();
    assert!(
        fact_units.len() <= 8,
        "the per-episode cap holds: {} facts",
        fact_units.len()
    );

    // The favorite-tea subject deduped to its LAST value within the episode.
    let tea_key = derive_subject_key(
        scope.as_uuid(),
        Some("preference"),
        Some("favorite tea"),
        "",
    );
    let tea_units: Vec<_> = units
        .iter()
        .filter(|unit| unit.subject_key.as_deref() == Some(tea_key.as_str()))
        .collect();
    assert_eq!(
        tea_units.len(),
        1,
        "favorite tea deduped within the episode"
    );
    assert!(
        tea_units[0].body.contains("oolong"),
        "the later assertion (oolong) wins: {}",
        tea_units[0].body
    );
}

/// §2 subject-key stability: the SAME subject asserted with DIFFERENT values in
/// DIFFERENT episodes derives the SAME subject key — the precondition for the
/// supersedence chain to fire.
#[tokio::test]
async fn same_subject_different_episodes_share_subject_key() {
    let store = InMemoryStore::default();
    let svc = service(store.clone(), true);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s1]\nuser: My name is Alexander Hamilton today.\n",
    )
    .await;
    retain_and_reflect(
        &svc,
        tenant,
        scope,
        actor,
        "[session s2]\nuser: My name is Alexander the Great now.\n",
    )
    .await;

    let name_key = derive_subject_key(scope.as_uuid(), Some("attribute"), Some("name"), "");
    let units = store.memory_units(tenant);
    let name_units: Vec<_> = units
        .iter()
        .filter(|unit| unit.subject_key.as_deref() == Some(name_key.as_str()))
        .collect();
    assert_eq!(
        name_units.len(),
        2,
        "both generations share the one subject key"
    );
    assert_eq!(
        name_units
            .iter()
            .filter(|unit| unit.state == UnitState::Active)
            .count(),
        1,
        "the later name supersedes the earlier one"
    );
}

/// §2 date coupling: with BOTH fact extraction and temporal grounding on, the
/// fact body carries the `[date ...]` prefix from the episode's parsed content
/// date; with temporal grounding OFF (fact extraction still on) the body is the
/// bare sentence — the two flags are not coupled.
#[tokio::test]
async fn date_prefix_only_when_temporal_grounding_also_on() {
    let dated_body = "[session s1] [date 2023/05/30]\nuser: My favorite tea is chamomile today.\n";
    let tea_key_scope = |scope: ScopeId| {
        derive_subject_key(
            scope.as_uuid(),
            Some("preference"),
            Some("favorite tea"),
            "",
        )
    };

    // Temporal ON.
    let store = InMemoryStore::default();
    let svc = MemoryService::new(
        Arc::new(store.clone()),
        Arc::new(CLOCK),
        Arc::new(NoopEmbedding),
    )
    .with_fact_extraction_enabled(true)
    .with_temporal_grounding_enabled(true);
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();
    retain_and_reflect(&svc, tenant, scope, actor, dated_body).await;
    let key = tea_key_scope(scope);
    let units = store.memory_units(tenant);
    let fact = units
        .iter()
        .find(|unit| unit.subject_key.as_deref() == Some(key.as_str()))
        .expect("fact mined");
    assert!(
        fact.body.contains("[date 2023-05-30]"),
        "temporal on ⇒ fact body is date-prefixed: {}",
        fact.body
    );
    assert!(fact.body.contains("chamomile"));

    // Temporal OFF, fact extraction still ON.
    let store2 = InMemoryStore::default();
    let svc2 = MemoryService::new(
        Arc::new(store2.clone()),
        Arc::new(CLOCK),
        Arc::new(NoopEmbedding),
    )
    .with_fact_extraction_enabled(true);
    let tenant2 = TenantId::new();
    let scope2 = ScopeId::new();
    let actor2 = ActorId::new();
    retain_and_reflect(&svc2, tenant2, scope2, actor2, dated_body).await;
    let key2 = tea_key_scope(scope2);
    let units2 = store2.memory_units(tenant2);
    let fact2 = units2
        .iter()
        .find(|unit| unit.subject_key.as_deref() == Some(key2.as_str()))
        .expect("fact mined");
    assert!(
        !fact2.body.contains("[date "),
        "temporal off ⇒ bare sentence, no date prefix: {}",
        fact2.body
    );
}
