use std::collections::HashMap;

use memphant_core::{CoreError, InMemoryStore, MemoryStore, recall, record_mark};
use memphant_types::{
    ActorId, ContextualChunk, LearnedRerankProfile, MarkOutcome, MarkRequest, MemoryEdgeKind,
    MemoryKind, NewEpisode, NewMemoryEdge, NewMemoryUnit, RecallChannel, RecallDropReason,
    RecallMode, RecallRequest, ScopeId, TenantId, TraceId, TrustLevel, UnitId, UnitState,
};
use serde::Deserialize;

fn tenant(value: u128) -> TenantId {
    TenantId::from_u128(value)
}

fn scope(value: u128) -> ScopeId {
    ScopeId::from_u128(value)
}

fn actor(value: u128) -> ActorId {
    ActorId::from_u128(value)
}

#[tokio::test]
async fn recall_writes_trace_for_scope_denial() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(70_000);
    let allowed_scope = scope(70_001);
    let denied_scope = scope(70_002);
    let actor_id = actor(70_003);

    let error = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id: denied_scope,
            actor_id,
            allowed_scope_ids: vec![allowed_scope],
            query: "Which callback version is current?".to_string(),
            k: 3,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-wsc-test".to_string(),
        },
    )
    .await
    .expect_err("denied recall returns a policy error");

    assert!(matches!(error, CoreError::PolicyDenied(_)));
    let traces = store.retrieval_traces(tenant_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].scope_id, denied_scope);
    assert_eq!(traces[0].policy_filters[0].reason, RecallDropReason::Scope);
    assert!(traces[0].context_items.is_empty());
    assert!(traces[0].abstention_signal);
}

#[tokio::test]
async fn dsr_decay_fold_promotes_reinforced_memory_over_ignored_stale_candidate() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(71_000);
    let scope_id = scope(71_001);
    let actor_id = actor(71_002);

    let mut tx = store.begin().await;
    let stale_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("deploy runbook current".to_string()),
                body: "Aardvark deploy runbook says to restart the legacy queue.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: Some("slow".to_string()),
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("stale unit seeded");
    let durable_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("deploy runbook current".to_string()),
                body: "Zulu deploy runbook says to run the atlas cutover checklist.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: Some("stable".to_string()),
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("durable unit seeded");
    store.commit(tx).await.expect("seed committed");

    for index in 0..3 {
        record_mark(
            &store,
            MarkRequest {
                tenant_id,
                trace_id: TraceId::from_u128(71_100 + index),
                caller_id: format!("rung11-positive-{index}"),
                used_ids: vec![durable_id],
                outcome: MarkOutcome::Success,
            },
        )
        .await
        .expect("positive review recorded");
    }
    record_mark(
        &store,
        MarkRequest {
            tenant_id,
            trace_id: TraceId::from_u128(71_200),
            caller_id: "rung11-negative".to_string(),
            used_ids: vec![stale_id],
            outcome: MarkOutcome::Ignored,
        },
    )
    .await
    .expect("negative review recorded");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "Which deploy runbook is current?".to_string(),
            k: 1,
            budget_tokens: 80,
            mode: RecallMode::Balanced,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung11-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert_eq!(response.candidate_whitelist, vec![durable_id]);
    let trace = store
        .trace_by_id(response.trace_id)
        .expect("trace recorded for decay fold");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "decay_enabled")
    );
    let trace_json = serde_json::to_value(&trace).expect("trace json");
    assert_eq!(trace_json["decay_model_id"], "fixed-prior-dsr-v1");
    let durable_candidate = trace_json["candidates"]
        .as_array()
        .expect("candidate array")
        .iter()
        .find(|candidate| candidate["unit_id"] == durable_id.as_uuid().to_string())
        .expect("durable candidate traced");
    let stale_candidate = trace_json["candidates"]
        .as_array()
        .expect("candidate array")
        .iter()
        .find(|candidate| candidate["unit_id"] == stale_id.as_uuid().to_string())
        .expect("stale candidate traced");
    assert!(
        durable_candidate["decay_retrievability"]
            .as_f64()
            .expect("durable retrievability")
            > stale_candidate["decay_retrievability"]
                .as_f64()
                .expect("stale retrievability")
    );
    assert_eq!(durable_candidate["dsr_reinforcement_count"], 3);
}

#[tokio::test]
async fn exhaustive_mode_gathers_buried_raw_episode_evidence_without_changing_fast_mode() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(71_500);
    let scope_id = scope(71_501);
    let actor_id = actor(71_502);

    let mut tx = store.begin().await;
    let decoy_episode = store
        .stage_episode(
            &mut tx,
            NewEpisode {
                tenant_id,
                scope_id,
                actor_id,
                source_kind: "fixture".to_string(),
                source_trust: TrustLevel::TrustedSystem,
                dedup_key: "rung12-decoy".to_string(),
                body: "Stargate deploy requires a routine restart.".to_string(),
            },
        )
        .await
        .expect("decoy episode seeded");
    let decoy_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("stargate deploy".to_string()),
                body: "Stargate deploy requires a routine restart.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: Some(decoy_episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("decoy unit seeded");
    let answer_episode = store
        .stage_episode(
            &mut tx,
            NewEpisode {
                tenant_id,
                scope_id,
                actor_id,
                source_kind: "fixture".to_string(),
                source_trust: TrustLevel::TrustedSystem,
                dedup_key: "rung12-answer".to_string(),
                body: "The archived raw episode says Stargate deploy was blocked until heliotrope approval landed.".to_string(),
            },
        )
        .await
        .expect("answer episode seeded");
    let answer_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("approval codename".to_string()),
                body: "Heliotrope.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: Some(answer_episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("answer unit seeded");
    store.commit(tx).await.expect("seed committed");

    let query = "What is required before Stargate deploy?".to_string();
    let fast = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: query.clone(),
            k: 1,
            budget_tokens: 20,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung12-test".to_string(),
        },
    )
    .await
    .expect("fast recall succeeds");

    assert_eq!(fast.candidate_whitelist, vec![decoy_id]);
    assert!(!fast.candidate_whitelist.contains(&answer_id));

    let exhaustive = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query,
            k: 1,
            budget_tokens: 20,
            mode: RecallMode::Exhaustive,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung12-test".to_string(),
        },
    )
    .await
    .expect("exhaustive recall succeeds");

    assert_eq!(exhaustive.candidate_whitelist, vec![answer_id]);
    assert_eq!(exhaustive.items[0].inclusion_reason, "l4_exhaustive");

    let trace = store
        .trace_by_id(exhaustive.trace_id)
        .expect("trace recorded for exhaustive recall");
    assert_eq!(trace.mode_requested, RecallMode::Exhaustive);
    assert_eq!(trace.mode_executed, RecallMode::Exhaustive);
    assert_eq!(trace.escalation_reason, "none");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "l4_exhaustive_enabled")
    );
    assert!(trace.iterative_scan_depth.unwrap_or_default() > 1);
    assert_eq!(
        trace.l4_sandbox_id.as_deref(),
        Some("deterministic-local-l4-v1")
    );
    assert!(
        trace
            .l4_gathered_evidence_ids
            .iter()
            .any(|evidence_id| evidence_id
                .contains(&answer_episode.episode_id.as_uuid().to_string()))
    );
    assert!(trace.candidates.iter().any(|candidate| {
        candidate.unit_id == answer_id && candidate.channel == RecallChannel::Exhaustive
    }));
}

#[tokio::test]
async fn contextual_chunk_recall_finds_source_unit_and_traces_flag() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(72_000);
    let scope_id = scope(72_001);
    let actor_id = actor(72_002);

    let mut tx = store.begin().await;
    let episode = store
        .stage_episode(
            &mut tx,
            NewEpisode {
                tenant_id,
                scope_id,
                actor_id,
                source_kind: "system".to_string(),
                source_trust: TrustLevel::TrustedSystem,
                dedup_key: "chunked-runbook".to_string(),
                body: "The deployment runbook mentions an emergency breaker named albatross."
                    .to_string(),
            },
        )
        .await
        .expect("episode seeded");
    let unit_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("deployment runbook".to_string()),
                body: "Runbook contains a gated switch.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("system".to_string()),
                source_episode_id: Some(episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: vec![ContextualChunk {
                    id: "chunk-albatross-breaker".to_string(),
                    header: "Deployment runbook / emergency breaker".to_string(),
                    body: "The emergency breaker codeword is albatross.".to_string(),
                    source_span: Some("episode:0-74".to_string()),
                }],
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("unit seeded");
    store.commit(tx).await.expect("seed committed");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is the albatross codeword?".to_string(),
            k: 1,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-ws4-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].unit_id, unit_id);
    assert_eq!(response.items[0].inclusion_reason, "contextual_chunk");

    let traces = store.retrieval_traces(tenant_id);
    assert_eq!(traces.len(), 1);
    assert!(
        traces[0]
            .feature_flags
            .iter()
            .any(|flag| flag == "contextual_chunks_enabled")
    );
    assert!(traces[0].candidates.iter().any(|candidate| {
        candidate.unit_id == unit_id
            && candidate.channel == RecallChannel::Vector
            && candidate.channel_score > 0.0
    }));
}

#[tokio::test]
async fn servicenow_query_does_not_trigger_temporal_recency_match() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(73_000);
    let scope_id = scope(73_001);
    let actor_id = actor(73_002);

    let mut tx = store.begin().await;
    let unit_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: None,
                body: "zzqv mrpl ntnk".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("unit seeded");
    store.commit(tx).await.expect("seed committed");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "I am working with our ServiceNow portal".to_string(),
            k: 8,
            budget_tokens: 256,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-temporal-token-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert!(!response.candidate_whitelist.contains(&unit_id));
}

#[tokio::test]
async fn recall_drops_expired_validity_window_for_current_query() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(74_000);
    let scope_id = scope(74_001);
    let actor_id = actor(74_002);

    let mut tx = store.begin().await;
    let stale_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("launch review office".to_string()),
                body: "Launch review office is Seattle.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: Some("2026-01-01T00:00:00Z".to_string()),
                valid_to: Some("2026-06-01T00:00:00Z".to_string()),
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("stale unit seeded");
    let current_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("launch review office".to_string()),
                body: "Launch review office is Taipei.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: Some("2026-06-01T00:00:00Z".to_string()),
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("current unit seeded");
    store.commit(tx).await.expect("seed committed");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "Which office is current for the launch review?".to_string(),
            k: 8,
            budget_tokens: 256,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung5-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert!(response.candidate_whitelist.contains(&current_id));
    assert!(!response.candidate_whitelist.contains(&stale_id));
    let trace = store
        .retrieval_traces(tenant_id)
        .into_iter()
        .next()
        .expect("trace recorded");
    assert!(
        trace
            .dropped_items
            .iter()
            .any(|item| { item.unit_id == stale_id && item.reason == RecallDropReason::Stale })
    );
}

#[tokio::test]
async fn edge_expansion_can_be_disabled_and_traces_related_candidates() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(75_000);
    let scope_id = scope(75_001);
    let actor_id = actor(75_002);

    let mut tx = store.begin().await;
    let anchor_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Resource,
                state: UnitState::Active,
                subject_key: Some("atlas pipeline".to_string()),
                body: "Atlas pipeline points to the sealed runbook.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("anchor seeded");
    let related_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("sealed runbook payload".to_string()),
                body: "Bluebird.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("related seeded");
    store
        .stage_memory_edge(
            &mut tx,
            NewMemoryEdge {
                tenant_id,
                scope_id,
                src_id: anchor_id,
                dst_id: related_id,
                kind: MemoryEdgeKind::DependsOn,
            },
        )
        .await
        .expect("edge seeded");
    store.commit(tx).await.expect("seed committed");

    let disabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is related to Atlas pipeline?".to_string(),
            k: 2,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: false,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung6-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds with edges disabled");
    assert!(!disabled.candidate_whitelist.contains(&related_id));

    let enabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is related to Atlas pipeline?".to_string(),
            k: 2,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung6-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds with edges enabled");

    assert!(enabled.candidate_whitelist.contains(&related_id));
    let trace = store.trace_by_id(enabled.trace_id).expect("trace exists");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "edge_expansion_enabled")
    );
    assert!(trace.candidates.iter().any(|candidate| {
        candidate.unit_id == related_id
            && candidate.channel == RecallChannel::Edge
            && candidate.channel_score > 0.0
    }));
}

#[tokio::test]
async fn packing_collapses_duplicate_decoys_and_preserves_answer_under_budget() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(76_000);
    let scope_id = scope(76_001);
    let actor_id = actor(76_002);

    let mut tx = store.begin().await;
    let mut seeded = Vec::new();
    for index in 1..=4 {
        let unit_id = store
            .stage_memory_unit(
                &mut tx,
                NewMemoryUnit {
                    tenant_id,
                    scope_id,
                    kind: MemoryKind::Semantic,
                    state: UnitState::Active,
                    subject_key: Some("prod deploy step".to_string()),
                    body: format!("A prod deploy step ran before release {index}."),
                    trust_level: TrustLevel::TrustedSystem,
                    churn_class: None,
                    freshness_due: false,
                    actor_id: Some(actor_id),
                    source_kind: Some("fixture".to_string()),
                    source_episode_id: None,
                    source_resource_id: None,
                    deletion_generation: None,
                    contextual_chunks: Vec::new(),
                    valid_from: None,
                    valid_to: None,
                    transaction_from: None,
                    transaction_to: None,
                },
            )
            .await
            .expect("decoy seeded");
        seeded.push(unit_id);
    }
    let answer_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("prod deploy approval".to_string()),
                body: "Prod deploy requires manual approval in release.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("answer seeded");
    store.commit(tx).await.expect("seed committed");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is required before prod deploy?".to_string(),
            k: 8,
            budget_tokens: 14,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung7-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert!(response.candidate_whitelist.contains(&answer_id));
    assert!(
        response
            .items
            .iter()
            .position(|item| item.unit_id == answer_id)
            .is_some_and(|position| position <= 1)
    );

    let trace = store.trace_by_id(response.trace_id).expect("trace exists");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "context_packing_abstention_enabled")
    );
    let duplicate_drops = trace
        .dropped_items
        .iter()
        .filter(|item| item.reason == RecallDropReason::Duplicate && seeded.contains(&item.unit_id))
        .count();
    assert!(duplicate_drops >= 3);
    assert!(!trace.abstention_signal);
}

#[tokio::test]
async fn packing_abstains_when_top_evidence_is_unresolved_contradiction() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(77_000);
    let scope_id = scope(77_001);
    let actor_id = actor(77_002);

    let mut tx = store.begin().await;
    let old_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("refund window".to_string()),
                body: "Refund window is 30 days.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("old unit seeded");
    let new_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("refund window policy".to_string()),
                body: "Refund window is 14 days.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("new unit seeded");
    store
        .stage_memory_edge(
            &mut tx,
            NewMemoryEdge {
                tenant_id,
                scope_id,
                src_id: old_id,
                dst_id: new_id,
                kind: MemoryEdgeKind::Contradicts,
            },
        )
        .await
        .expect("contradiction edge seeded");
    store.commit(tx).await.expect("seed committed");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is the refund window?".to_string(),
            k: 4,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung7-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert!(response.candidate_whitelist.contains(&old_id));
    assert!(response.candidate_whitelist.contains(&new_id));
    assert!(response.abstention);
    assert!(
        response
            .suppression_labels
            .iter()
            .any(|label| label == "unresolved_contradiction")
    );
}

#[tokio::test]
async fn bounded_rerank_reorders_rank_sensitive_candidate_and_traces_decision() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(78_000);
    let scope_id = scope(78_001);
    let actor_id = actor(78_002);

    let mut tx = store.begin().await;
    let decoy_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("pager alerts".to_string()),
                body: "Owner currently resolves pager alerts noise.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("decoy seeded");
    let answer_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("incident owner".to_string()),
                body: "Incident owner resolves pager alerts.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("answer seeded");
    store.commit(tx).await.expect("seed committed");

    let disabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "Which owner currently resolves pager alerts?".to_string(),
            k: 1,
            budget_tokens: 64,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: false,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung8-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds with rerank disabled");
    let disabled_trace = store.trace_by_id(disabled.trace_id).expect("trace exists");
    assert!(
        disabled.candidate_whitelist.contains(&decoy_id),
        "disabled whitelist={:?}, decoy_id={:?}, answer_id={:?}, candidates={:?}",
        disabled.candidate_whitelist,
        decoy_id,
        answer_id,
        disabled_trace.candidates
    );
    assert!(!disabled.candidate_whitelist.contains(&answer_id));

    let enabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "Which owner currently resolves pager alerts?".to_string(),
            k: 1,
            budget_tokens: 64,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung8-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds with rerank enabled");
    assert!(enabled.candidate_whitelist.contains(&answer_id));
    assert!(!enabled.candidate_whitelist.contains(&decoy_id));

    let trace = store.trace_by_id(enabled.trace_id).expect("trace exists");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "rerank_enabled")
    );
    assert_eq!(trace.reranker_id, "deterministic-local-v1");
    assert!(trace.rerank_input_count >= 2);
    assert!(trace.rerank_overfetch_ratio >= 2.0);

    let answer_trace = trace
        .candidates
        .iter()
        .find(|candidate| candidate.unit_id == answer_id)
        .expect("answer candidate traced");
    let decoy_trace = trace
        .candidates
        .iter()
        .find(|candidate| candidate.unit_id == decoy_id)
        .expect("decoy candidate traced");
    assert_eq!(answer_trace.rerank_rank, Some(1));
    assert!(decoy_trace.rerank_rank.is_some_and(|rank| rank > 1));
    assert!(answer_trace.rerank_score > decoy_trace.rerank_score);
}

#[tokio::test]
async fn learned_rerank_profile_reorders_protected_topk_and_traces_training_set() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(78_100);
    let scope_id = scope(78_101);
    let actor_id = actor(78_102);

    let mut tx = store.begin().await;
    let decoy_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("atlas rollback chatter".to_string()),
                body: "Atlas rollback should use the noisy rollback runbook notes that repeat atlas rollback runbook terms but do not name the canonical runbook.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("decoy seeded");
    let answer_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("atlas rollback runbook".to_string()),
                body: "Use the mira-ledger recovery runbook.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("answer seeded");
    store.commit(tx).await.expect("seed committed");

    let query = "Which atlas rollback runbook should we use?";
    let deterministic = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: query.to_string(),
            k: 1,
            budget_tokens: 64,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung13-test".to_string(),
        },
    )
    .await
    .expect("deterministic recall succeeds");
    assert!(deterministic.candidate_whitelist.contains(&decoy_id));
    assert!(!deterministic.candidate_whitelist.contains(&answer_id));

    let learned = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: query.to_string(),
            k: 1,
            budget_tokens: 64,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: Some(LearnedRerankProfile {
                profile_id: "memory-tuned-linear-rung13-v1".to_string(),
                training_set_id: "rung13_learned_rerank_training_001".to_string(),
                lexical_weight: 0.2,
                vector_weight: 0.2,
                exact_weight: 8.0,
                intent_weight: 0.0,
                decay_weight: 0.5,
                fused_weight: 0.2,
            }),
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung13-test".to_string(),
        },
    )
    .await
    .expect("learned-profile recall succeeds");
    assert!(learned.candidate_whitelist.contains(&answer_id));
    assert!(!learned.candidate_whitelist.contains(&decoy_id));

    let trace = store.trace_by_id(learned.trace_id).expect("trace exists");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "learned_rerank_enabled")
    );
    assert_eq!(trace.reranker_id, "memory-tuned-linear-rung13-v1");
    assert_eq!(trace.weight_vector_id, "memory-tuned-linear-rung13-v1");
    assert_eq!(
        trace.learned_rerank_training_set_id.as_deref(),
        Some("rung13_learned_rerank_training_001")
    );

    let answer_trace = trace
        .candidates
        .iter()
        .find(|candidate| candidate.unit_id == answer_id)
        .expect("answer candidate traced");
    let decoy_trace = trace
        .candidates
        .iter()
        .find(|candidate| candidate.unit_id == decoy_id)
        .expect("decoy candidate traced");
    assert_eq!(answer_trace.rerank_rank, Some(1));
    assert!(decoy_trace.rerank_rank.is_some_and(|rank| rank > 1));
    assert!(answer_trace.rerank_score > decoy_trace.rerank_score);
}

#[tokio::test]
async fn query_decomposition_recovers_composite_answer_and_traces_subqueries() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(79_000);
    let scope_id = scope(79_001);
    let actor_id = actor(79_002);

    let mut tx = store.begin().await;
    let file_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("deploy task file".to_string()),
                body: "Deploy task file changed rollout.toml.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("file unit seeded");
    let approval_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("release approval".to_string()),
                body: "Manual gate is required.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("approval unit seeded");
    let mut decoy_ids = Vec::new();
    for index in 0..4 {
        let decoy_id = store
            .stage_memory_unit(
                &mut tx,
                NewMemoryUnit {
                    tenant_id,
                    scope_id,
                    kind: MemoryKind::Semantic,
                    state: UnitState::Active,
                    subject_key: Some(format!("release approval chatter {index}")),
                    body: format!(
                        "Deploy task file changed release approval required noisy status {index}."
                    ),
                    trust_level: TrustLevel::TrustedSystem,
                    churn_class: None,
                    freshness_due: false,
                    actor_id: Some(actor_id),
                    source_kind: Some("fixture".to_string()),
                    source_episode_id: None,
                    source_resource_id: None,
                    deletion_generation: None,
                    contextual_chunks: Vec::new(),
                    valid_from: None,
                    valid_to: None,
                    transaction_from: None,
                    transaction_to: None,
                },
            )
            .await
            .expect("decoy unit seeded");
        decoy_ids.push(decoy_id);
    }
    store.commit(tx).await.expect("seed committed");

    let query = "Which deploy task file changed and which release approval is required?";
    let disabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: query.to_string(),
            k: 2,
            budget_tokens: 96,
            mode: RecallMode::Balanced,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: false,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung9-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds with decomposition disabled");
    let disabled_trace = store.trace_by_id(disabled.trace_id).expect("trace exists");
    assert!(
        decoy_ids
            .iter()
            .any(|decoy_id| disabled.candidate_whitelist.contains(decoy_id)),
        "disabled whitelist={:?}, decoy_ids={:?}",
        disabled.candidate_whitelist,
        decoy_ids
    );
    assert!(
        !disabled.candidate_whitelist.contains(&approval_id),
        "disabled whitelist should miss release approval, got {:?}; file_id={:?}; approval_id={:?}; candidates={:?}",
        disabled.candidate_whitelist,
        file_id,
        approval_id,
        disabled_trace.candidates
    );

    let enabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: query.to_string(),
            k: 2,
            budget_tokens: 96,
            mode: RecallMode::Balanced,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung9-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds with decomposition enabled");
    assert!(enabled.candidate_whitelist.contains(&file_id));
    assert!(enabled.candidate_whitelist.contains(&approval_id));

    let trace = store.trace_by_id(enabled.trace_id).expect("trace exists");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "query_decomposition_enabled")
    );
    assert!(trace.subquery_ids.len() >= 2);
    assert!(
        trace
            .decomposition_reason
            .contains("multi_constraint_conjunction")
    );

    let approval_trace = trace
        .candidates
        .iter()
        .find(|candidate| candidate.unit_id == approval_id)
        .expect("approval candidate traced");
    assert!(!approval_trace.subquery_ids.is_empty());
}

#[derive(Debug, Deserialize)]
struct GoldenCase {
    id: String,
    query: String,
    #[serde(default)]
    budget_tokens: Option<usize>,
    expected_weight_vector_id: String,
    expected_stage_names: Vec<String>,
    #[serde(default)]
    expect_filter_selectivity_lt_one: bool,
    seed: GoldenSeed,
    expect: GoldenExpect,
}

#[derive(Debug, Deserialize)]
struct GoldenSeed {
    units: Vec<GoldenUnit>,
    #[serde(default)]
    edges: Vec<GoldenEdge>,
}

#[derive(Debug, Deserialize)]
struct GoldenUnit {
    name: String,
    #[serde(default = "primary_tenant")]
    tenant: String,
    #[serde(default = "primary_scope")]
    scope: String,
    episode_body: String,
    kind: MemoryKind,
    state: UnitState,
    subject_key: Option<String>,
    body: String,
    trust_level: TrustLevel,
    #[serde(default)]
    deletion_generation: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GoldenExpect {
    answer_bearing_units: Vec<String>,
    #[serde(default)]
    forbidden_units: Vec<String>,
    #[serde(default)]
    dropped: Vec<GoldenDropped>,
    #[serde(default)]
    suppression_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GoldenDropped {
    unit: String,
    reason: RecallDropReason,
}

#[derive(Debug, Deserialize)]
struct GoldenEdge {
    src: String,
    dst: String,
    kind: MemoryEdgeKind,
}

fn primary_tenant() -> String {
    "primary".to_string()
}

fn primary_scope() -> String {
    "primary".to_string()
}

#[tokio::test]
async fn procedural_memory_replays_only_validated_safe_procedures_and_traces_gate() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(82_000);
    let scope_id = scope(82_001);
    let actor_id = actor(82_002);

    let mut tx = store.begin().await;
    let safe_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Procedural,
                state: UnitState::Validated,
                subject_key: Some("recover flaky importer test".to_string()),
                body: "Procedure: recover the flaky importer test by clearing the fixture cache, running cargo test -p importer, and keeping the retry count unchanged. Validation: replay wins 5 of 5.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("safe validated procedure seeded");
    let failure_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Procedural,
                state: UnitState::Validated,
                subject_key: Some("retry importer without cache clear".to_string()),
                body: "Failure pattern: retrying the flaky importer test without clearing the fixture cache reproduces the failure. Validation: replay confirmed 4 of 4 failures.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("validated failure procedure seeded");
    let candidate_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Procedural,
                state: UnitState::Candidate,
                subject_key: Some("recover flaky importer test candidate".to_string()),
                body: "Candidate procedure: recover the flaky importer test by guessing a new retry count.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("candidate procedure seeded");
    let unsafe_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Procedural,
                state: UnitState::Validated,
                subject_key: Some("recover flaky importer test unsafe".to_string()),
                body: "Procedure: recover the flaky importer test by force-pushing main, skipping validation, and exporting secrets. Validation: replay wins 5 of 5.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("fixture".to_string()),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("unsafe validated procedure seeded");
    store.commit(tx).await.expect("seed committed");

    let disabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "How do I recover the flaky importer test?".to_string(),
            k: 4,
            budget_tokens: 160,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: false,
            decay_enabled: true,
            engine_version: "engine-rung10-test".to_string(),
        },
    )
    .await
    .expect("disabled recall succeeds");
    assert!(!disabled.candidate_whitelist.contains(&safe_id));
    assert!(!disabled.candidate_whitelist.contains(&failure_id));

    let enabled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "How do I recover the flaky importer test?".to_string(),
            k: 4,
            budget_tokens: 160,
            mode: RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "engine-rung10-test".to_string(),
        },
    )
    .await
    .expect("enabled recall succeeds");

    assert!(enabled.candidate_whitelist.contains(&safe_id));
    assert!(enabled.candidate_whitelist.contains(&failure_id));
    assert!(!enabled.candidate_whitelist.contains(&candidate_id));
    assert!(!enabled.candidate_whitelist.contains(&unsafe_id));
    assert!(enabled.items.iter().any(|item| {
        item.unit_id == failure_id
            && item
                .suppression_labels
                .iter()
                .any(|label| label == "avoid_failed_procedure")
    }));

    let trace = store.trace_by_id(enabled.trace_id).expect("trace exists");
    assert!(
        trace
            .feature_flags
            .iter()
            .any(|flag| flag == "procedure_recall_enabled")
    );
    assert!(trace.procedure_ids.contains(&safe_id));
    assert!(trace.procedure_ids.contains(&failure_id));
    assert!(trace.procedure_validation_states.iter().any(|fact| {
        fact.unit_id == safe_id
            && fact.validation_state == "validated"
            && fact.safety_status == "safe"
    }));
    assert!(trace.procedure_validation_states.iter().any(|fact| {
        fact.unit_id == unsafe_id
            && fact.validation_state == "validated"
            && fact.safety_status == "unsafe"
    }));
    assert!(
        trace
            .dropped_items
            .iter()
            .any(|item| { item.unit_id == candidate_id && item.reason == RecallDropReason::State })
    );
    assert!(trace.dropped_items.iter().any(|item| {
        item.unit_id == unsafe_id && item.reason == RecallDropReason::ProtectedCategory
    }));
}

#[tokio::test]
async fn recall_golden_fixtures_pass() {
    let cases: Vec<GoldenCase> = serde_json::from_str(include_str!(
        "../../../examples/evals/wsc-recall-goldens.json"
    ))
    .expect("fixtures parse");

    for case in cases {
        let store = InMemoryStore::default();
        let tenant_id = tenant(71_000);
        let scope_id = scope(71_001);
        let denied_scope_id = scope(71_003);
        let other_tenant_id = tenant(71_004);
        let actor_id = actor(71_002);
        let mut named_units: HashMap<String, UnitId> = HashMap::new();

        let mut tx = store.begin().await;
        for unit in &case.seed.units {
            let unit_tenant_id = if unit.tenant == "other" {
                other_tenant_id
            } else {
                tenant_id
            };
            let unit_scope_id = if unit.scope == "denied" {
                denied_scope_id
            } else {
                scope_id
            };
            let episode = store
                .stage_episode(
                    &mut tx,
                    NewEpisode {
                        tenant_id: unit_tenant_id,
                        scope_id: unit_scope_id,
                        actor_id,
                        source_kind: "system".to_string(),
                        source_trust: unit.trust_level,
                        dedup_key: format!("{}:{}", case.id, unit.name),
                        body: unit.episode_body.clone(),
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("{} episode seed failed: {error}", case.id));
            let unit_id = store
                .stage_memory_unit(
                    &mut tx,
                    NewMemoryUnit {
                        tenant_id: unit_tenant_id,
                        scope_id: unit_scope_id,
                        kind: unit.kind,
                        state: unit.state,
                        subject_key: unit.subject_key.clone(),
                        body: unit.body.clone(),
                        trust_level: unit.trust_level,
                        churn_class: None,
                        freshness_due: false,
                        actor_id: Some(actor_id),
                        source_kind: Some("system".to_string()),
                        source_episode_id: Some(episode.episode_id),
                        source_resource_id: None,
                        deletion_generation: unit.deletion_generation,
                        contextual_chunks: Vec::new(),
                        valid_from: None,
                        valid_to: None,
                        transaction_from: None,
                        transaction_to: None,
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("{} unit seed failed: {error}", case.id));
            named_units.insert(unit.name.clone(), unit_id);
        }
        for edge in &case.seed.edges {
            store
                .stage_memory_edge(
                    &mut tx,
                    NewMemoryEdge {
                        tenant_id,
                        scope_id,
                        src_id: *named_units
                            .get(&edge.src)
                            .unwrap_or_else(|| panic!("{} missing edge src {}", case.id, edge.src)),
                        dst_id: *named_units
                            .get(&edge.dst)
                            .unwrap_or_else(|| panic!("{} missing edge dst {}", case.id, edge.dst)),
                        kind: edge.kind,
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("{} edge seed failed: {error}", case.id));
        }
        store
            .commit(tx)
            .await
            .unwrap_or_else(|error| panic!("{} seed commit failed: {error}", case.id));

        let response = recall(
            &store,
            RecallRequest {
                tenant_id,
                scope_id,
                actor_id,
                allowed_scope_ids: vec![scope_id],
                query: case.query.clone(),
                k: 3,
                budget_tokens: case.budget_tokens.unwrap_or(80),
                mode: RecallMode::Fast,
                include_beliefs: false,
                edge_expansion_enabled: true,
                context_packing_abstention_enabled: true,
                rerank_enabled: true,
                learned_rerank_profile: None,
                query_decomposition_enabled: true,
                procedure_recall_enabled: true,
                decay_enabled: true,
                engine_version: "engine-wsc-test".to_string(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("{} recall failed: {error}", case.id));

        for expected_name in &case.expect.answer_bearing_units {
            let expected_id = named_units
                .get(expected_name)
                .unwrap_or_else(|| panic!("{} missing unit {expected_name}", case.id));
            assert!(
                response.candidate_whitelist.contains(expected_id),
                "{} whitelist contains {}",
                case.id,
                expected_name
            );
        }
        for forbidden_name in &case.expect.forbidden_units {
            let forbidden_id = named_units
                .get(forbidden_name)
                .unwrap_or_else(|| panic!("{} missing unit {forbidden_name}", case.id));
            assert!(
                !response.candidate_whitelist.contains(forbidden_id),
                "{} whitelist excludes {}",
                case.id,
                forbidden_name
            );
        }
        assert!(
            response
                .citations
                .iter()
                .all(|citation| response.candidate_whitelist.contains(&citation.unit_id)),
            "{} citations stay inside candidate whitelist",
            case.id
        );
        assert!(
            response
                .citations
                .iter()
                .any(|citation| citation.episode_id.is_some()),
            "{} emits source episode citation",
            case.id
        );

        let traces = store.retrieval_traces(tenant_id);
        let trace = traces
            .last()
            .unwrap_or_else(|| panic!("{} missing retrieval trace", case.id));
        let stage_names: Vec<_> = trace
            .channel_runs
            .iter()
            .map(|stage| stage.stage.clone())
            .collect();
        assert_eq!(stage_names, case.expected_stage_names, "{}", case.id);
        assert_eq!(
            trace.weight_vector_id, case.expected_weight_vector_id,
            "{}",
            case.id
        );
        if case.expect_filter_selectivity_lt_one {
            assert!(
                trace.filter_selectivity.unwrap_or(1.0) < 1.0,
                "{} filter_selectivity shows shared-corpus filtering",
                case.id
            );
        }
        for expected_drop in &case.expect.dropped {
            let dropped_id = named_units.get(&expected_drop.unit).unwrap_or_else(|| {
                panic!("{} missing dropped unit {}", case.id, expected_drop.unit)
            });
            assert!(
                trace
                    .dropped_items
                    .iter()
                    .any(|item| item.unit_id == *dropped_id && item.reason == expected_drop.reason),
                "{} trace drops {} as {:?}",
                case.id,
                expected_drop.unit,
                expected_drop.reason
            );
        }
        for expected_label in &case.expect.suppression_labels {
            assert!(
                response
                    .suppression_labels
                    .iter()
                    .any(|label| label == expected_label),
                "{} response includes suppression label {}",
                case.id,
                expected_label
            );
        }
    }
}
