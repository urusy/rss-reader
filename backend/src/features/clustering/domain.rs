//! Clustering pure logic: similarity banding, union-find grouping, representative
//! selection, signature, and summary-input building. No DB/LLM → unit-tested.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimilarityBand {
    Duplicate,
    SameTopic,
    Unrelated,
}

pub fn classify_similarity(sim: f32, topic_threshold: f32, dup_threshold: f32) -> SimilarityBand {
    if sim >= dup_threshold {
        SimilarityBand::Duplicate
    } else if sim >= topic_threshold {
        SimilarityBand::SameTopic
    } else {
        SimilarityBand::Unrelated
    }
}

/// Normalize a title before display/representative use (case + whitespace).
pub fn normalize_title(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Group nodes into connected components by edges >= threshold (union-find).
/// Deterministic: each group preserves the input order of `nodes`.
pub fn group_edges(nodes: &[Uuid], edges: &[(Uuid, Uuid, f32)], threshold: f32) -> Vec<Vec<Uuid>> {
    let index: HashMap<Uuid, usize> = nodes.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut parent: Vec<usize> = (0..nodes.len()).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    for (a, b, sim) in edges {
        if *sim < threshold {
            continue;
        }
        let (Some(&ia), Some(&ib)) = (index.get(a), index.get(b)) else {
            continue;
        };
        let ra = find(&mut parent, ia);
        let rb = find(&mut parent, ib);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    let mut groups: HashMap<usize, Vec<Uuid>> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    for (i, id) in nodes.iter().enumerate() {
        let r = find(&mut parent, i);
        if !groups.contains_key(&r) {
            order.push(r);
        }
        groups.entry(r).or_default().push(*id);
    }
    order
        .into_iter()
        .map(|r| groups.remove(&r).unwrap())
        .collect()
}

#[derive(Debug, Clone)]
pub struct MemberCandidate {
    pub article_id: Uuid,
    pub title: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Pick the representative: oldest published (the first report), NULLs last;
/// tie → longer title; tie → uuid asc. Deterministic.
pub fn pick_representative(members: &[MemberCandidate]) -> Uuid {
    members
        .iter()
        .min_by(|a, b| {
            let ka = (a.published_at.is_none(), a.published_at);
            let kb = (b.published_at.is_none(), b.published_at);
            ka.cmp(&kb)
                .then(b.title.chars().count().cmp(&a.title.chars().count()))
                .then(a.article_id.cmp(&b.article_id))
        })
        .map(|m| m.article_id)
        .expect("cluster has at least one member")
}

/// Stable fingerprint of an (order-independent) article-id set. Used to carry
/// over a cached summary across rebuilds.
pub fn cluster_signature(member_ids: &[Uuid]) -> String {
    let mut ids: Vec<Uuid> = member_ids.to_vec();
    ids.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for id in &ids {
        id.hash(&mut hasher);
    }
    format!("{:016x}-{}", hasher.finish(), ids.len())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClusterMemberSource {
    pub feed_title: Option<String>,
    pub title: String,
    pub snippet: String,
}

pub fn build_cluster_summary_input(members: &[ClusterMemberSource]) -> String {
    members
        .iter()
        .map(|m| {
            let source = m.feed_title.as_deref().unwrap_or("(unknown source)").trim();
            format!("- 【{}】{}: {}", source, m.title.trim(), m.snippet.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Cluster {
    pub id: Uuid,
    pub title: String,
    pub size: i32,
    pub summary: Option<String>,
    pub summary_lang: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ClusterMember {
    pub cluster_id: Uuid,
    pub article_id: Uuid,
    pub title: String,
    pub url: String,
    pub feed_id: Uuid,
    pub feed_title: Option<String>,
    pub is_representative: bool,
    pub is_duplicate: bool,
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterWithMembers {
    #[serde(flatten)]
    pub cluster: Cluster,
    pub members: Vec<ClusterMember>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn classify_similarity_bands() {
        assert_eq!(
            classify_similarity(0.7, 0.3, 0.6),
            SimilarityBand::Duplicate
        );
        assert_eq!(
            classify_similarity(0.4, 0.3, 0.6),
            SimilarityBand::SameTopic
        );
        assert_eq!(
            classify_similarity(0.1, 0.3, 0.6),
            SimilarityBand::Unrelated
        );
    }

    #[test]
    fn normalize_title_lowercases_and_collapses() {
        assert_eq!(normalize_title("  Hello   World "), "hello world");
    }

    #[test]
    fn group_edges_merges_transitive() {
        let nodes = vec![uid(1), uid(2), uid(3), uid(4)];
        // 1-2, 2-3 connected; 4 alone.
        let edges = vec![(uid(1), uid(2), 0.5), (uid(2), uid(3), 0.4)];
        let groups = group_edges(&nodes, &edges, 0.3);
        assert_eq!(groups.len(), 2);
        let big = groups.iter().find(|g| g.len() == 3).unwrap();
        assert!(big.contains(&uid(1)) && big.contains(&uid(3)));
    }

    #[test]
    fn group_edges_ignores_below_threshold() {
        let nodes = vec![uid(1), uid(2)];
        let edges = vec![(uid(1), uid(2), 0.2)];
        let groups = group_edges(&nodes, &edges, 0.3);
        assert_eq!(groups.len(), 2); // not merged
    }

    #[test]
    fn pick_representative_prefers_oldest() {
        let t0 = chrono::DateTime::from_timestamp(1000, 0).unwrap();
        let t1 = chrono::DateTime::from_timestamp(2000, 0).unwrap();
        let members = vec![
            MemberCandidate {
                article_id: uid(2),
                title: "new".into(),
                published_at: Some(t1),
            },
            MemberCandidate {
                article_id: uid(1),
                title: "old".into(),
                published_at: Some(t0),
            },
        ];
        assert_eq!(pick_representative(&members), uid(1));
    }

    #[test]
    fn cluster_signature_is_order_independent() {
        assert_eq!(
            cluster_signature(&[uid(1), uid(2), uid(3)]),
            cluster_signature(&[uid(3), uid(1), uid(2)])
        );
        assert_ne!(
            cluster_signature(&[uid(1), uid(2)]),
            cluster_signature(&[uid(1), uid(3)])
        );
    }

    #[test]
    fn build_cluster_summary_input_formats() {
        let m = vec![ClusterMemberSource {
            feed_title: Some("Outlet".into()),
            title: "T".into(),
            snippet: "S".into(),
        }];
        assert_eq!(build_cluster_summary_input(&m), "- 【Outlet】T: S");
    }

    #[test]
    fn build_cluster_summary_input_unknown_source() {
        let m = vec![ClusterMemberSource {
            feed_title: None,
            title: "T".into(),
            snippet: "S".into(),
        }];
        assert!(build_cluster_summary_input(&m).contains("(unknown source)"));
    }
}
