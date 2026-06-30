use std::collections::HashMap;

use uuid::Uuid;

use super::domain::{
    build_cluster_summary_input, classify_similarity, cluster_signature, group_edges,
    normalize_title, pick_representative, Cluster, ClusterWithMembers, MemberCandidate,
    SimilarityBand,
};
use super::repository::{self, ClusterEdge, NewCluster, NewMember};
use crate::shared::error::{AppError, AppResult};
use crate::shared::llm::anthropic::AnthropicClient;
use crate::shared::llm::{ClusterSummaryRequest, LlmClient};
use crate::shared::state::AppState;

fn llm_client(state: &AppState) -> AppResult<AnthropicClient> {
    let key = state
        .config
        .anthropic_api_key
        .clone()
        .ok_or_else(|| AppError::NotEnabled("ANTHROPIC_API_KEY is not set".into()))?;
    Ok(AnthropicClient::new(
        state.http.clone(),
        key,
        state.config.anthropic_model.clone(),
    ))
}

fn ordered(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Recompute clusters from the recent window (trigram + union-find). No LLM call.
/// Returns the number of persisted clusters.
pub async fn recluster(state: &AppState) -> AppResult<usize> {
    let cfg = &state.config;
    let hours = cfg.clustering_window_hours;
    let cap = cfg.clustering_max_articles;

    let nodes = repository::recent_nodes(&state.db, hours, cap).await?;
    if nodes.is_empty() {
        repository::replace_clusters(&state.db, &[]).await?;
        return Ok(0);
    }

    let edges =
        repository::similarity_edges(&state.db, hours, cap, cfg.cluster_topic_threshold).await?;

    let mut sim_map: HashMap<(Uuid, Uuid), f32> = HashMap::new();
    for ClusterEdge {
        left_id,
        right_id,
        sim,
    } in &edges
    {
        sim_map.insert(ordered(*left_id, *right_id), *sim);
    }

    let node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();
    let edge_tuples: Vec<(Uuid, Uuid, f32)> = edges
        .iter()
        .map(|e| (e.left_id, e.right_id, e.sim))
        .collect();
    let groups = group_edges(&node_ids, &edge_tuples, cfg.cluster_topic_threshold);

    let carried: HashMap<String, (String, String, String)> =
        repository::existing_summaries(&state.db)
            .await?
            .into_iter()
            .filter_map(|(sig, s, l, m)| match (s, l, m) {
                (Some(s), Some(l), Some(m)) => Some((sig, (s, l, m))),
                _ => None,
            })
            .collect();

    let by_id: HashMap<Uuid, &repository::ArticleNode> = nodes.iter().map(|n| (n.id, n)).collect();

    let mut new_clusters: Vec<NewCluster> = Vec::new();
    for group in groups {
        if (group.len() as i32) < cfg.cluster_min_size {
            continue;
        }
        let candidates: Vec<MemberCandidate> = group
            .iter()
            .filter_map(|id| {
                by_id.get(id).map(|n| MemberCandidate {
                    article_id: n.id,
                    title: n.title.clone(),
                    published_at: n.published_at,
                })
            })
            .collect();
        let rep_id = pick_representative(&candidates);
        let rep_title = by_id
            .get(&rep_id)
            .map(|n| normalize_title(&n.title))
            .unwrap_or_default();

        let members: Vec<NewMember> = group
            .iter()
            .map(|&aid| {
                let sim = if aid == rep_id {
                    1.0
                } else {
                    *sim_map.get(&ordered(aid, rep_id)).unwrap_or(&0.0)
                };
                let is_dup = matches!(
                    classify_similarity(
                        sim,
                        cfg.cluster_topic_threshold,
                        cfg.cluster_dup_threshold
                    ),
                    SimilarityBand::Duplicate
                );
                NewMember {
                    article_id: aid,
                    is_representative: aid == rep_id,
                    is_duplicate: is_dup && aid != rep_id,
                    similarity: sim,
                }
            })
            .collect();

        let signature = cluster_signature(&group);
        let carried_summary = carried.get(&signature).cloned();

        new_clusters.push(NewCluster {
            title: rep_title,
            signature,
            members,
            carried_summary,
        });
    }

    let count = new_clusters.len();
    repository::replace_clusters(&state.db, &new_clusters).await?;
    Ok(count)
}

pub async fn list_clusters(state: &AppState) -> AppResult<Vec<ClusterWithMembers>> {
    let clusters = repository::list_clusters(&state.db, state.config.cluster_min_size).await?;
    if clusters.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<Uuid> = clusters.iter().map(|c| c.id).collect();
    let members = repository::members_for(&state.db, &ids).await?;

    let mut by_cluster: HashMap<Uuid, Vec<_>> = HashMap::new();
    for m in members {
        by_cluster.entry(m.cluster_id).or_default().push(m);
    }
    Ok(clusters
        .into_iter()
        .map(|c| {
            let members = by_cluster.remove(&c.id).unwrap_or_default();
            ClusterWithMembers {
                cluster: c,
                members,
            }
        })
        .collect())
}

pub async fn get_cluster(state: &AppState, id: Uuid) -> AppResult<ClusterWithMembers> {
    let cluster = repository::get_cluster(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let members = repository::members_for(&state.db, &[id]).await?;
    Ok(ClusterWithMembers { cluster, members })
}

/// Generate (or return cached) integrated summary. Order: cluster exists →
/// cache hit → LLM gate → generate.
pub async fn summarize_cluster(
    state: &AppState,
    id: Uuid,
    target_lang: &str,
) -> AppResult<Cluster> {
    let cluster = repository::get_cluster(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;

    if let (Some(s), Some(l)) = (&cluster.summary, &cluster.summary_lang) {
        if l == target_lang && !s.is_empty() {
            return Ok(cluster);
        }
    }

    let client = llm_client(state)?;
    let sources = repository::member_sources(&state.db, id).await?;
    let items = build_cluster_summary_input(&sources);
    let summary = client
        .cluster_summary(ClusterSummaryRequest {
            items,
            target_lang: target_lang.to_string(),
        })
        .await?;

    repository::save_summary(
        &state.db,
        id,
        &summary,
        target_lang,
        &state.config.anthropic_model,
    )
    .await?;
    repository::get_cluster(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)
}
