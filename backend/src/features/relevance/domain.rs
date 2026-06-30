//! Relevance scoring: parse/normalize LLM scores, build the interest profile,
//! and fingerprint it — all pure (no LLM/DB) → unit-tested.

use std::collections::HashSet;

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RelevanceScore {
    pub article_id: Uuid,
    pub score: f32,
    pub reasoning: Option<String>,
    pub scored_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoreResult {
    pub scored_count: usize,
    pub profile_hash: String,
    pub scores: Vec<RelevanceScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileView {
    pub profile: String,
    pub hash: String,
    pub tag_count: usize,
    pub read_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawScore {
    pub article_id: Uuid,
    pub score: f32,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct LlmScore {
    id: String,
    score: f32,
    #[serde(default)]
    reason: Option<String>,
}

/// Normalize a score to [0,1]. >1.0 (and <=100) is treated as a 0-100 scale.
/// Non-finite → 0.0.
pub fn normalize_score(raw: f32) -> f32 {
    if !raw.is_finite() {
        return 0.0;
    }
    let v = if raw > 1.0 { raw / 100.0 } else { raw };
    v.clamp(0.0, 1.0)
}

/// Parse the LLM's raw output into normalized scores, dropping hallucinated /
/// unknown ids and duplicates. Slices from first `[` to last `]`. Err = safe.
pub fn parse_relevance_scores(
    raw: &str,
    valid_ids: &HashSet<Uuid>,
) -> Result<Vec<RawScore>, String> {
    let start = raw.find('[').ok_or("no JSON array found in LLM output")?;
    let end = raw.rfind(']').ok_or("no JSON array found in LLM output")?;
    if end < start {
        return Err("malformed JSON array in LLM output".into());
    }
    let slice = &raw[start..=end];
    let parsed: Vec<LlmScore> =
        serde_json::from_str(slice).map_err(|e| format!("invalid score JSON: {e}"))?;

    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut out = Vec::new();
    for s in parsed {
        let Ok(id) = Uuid::parse_str(s.id.trim()) else {
            continue;
        };
        if !valid_ids.contains(&id) {
            continue;
        }
        if !seen.insert(id) {
            continue;
        }
        let reasoning = s
            .reason
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());
        out.push(RawScore {
            article_id: id,
            score: normalize_score(s.score),
            reasoning,
        });
    }
    Ok(out)
}

/// Build the interest-profile string from (tag, count) and recent read titles.
pub fn build_profile(tags: &[(String, i64)], read_titles: &[String]) -> String {
    if tags.is_empty() && read_titles.is_empty() {
        return "(no profile yet)".to_string();
    }
    let mut s = String::new();
    if !tags.is_empty() {
        let list = tags
            .iter()
            .map(|(name, count)| format!("{name} (x{count})"))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str("Frequently used tags (interests): ");
        s.push_str(&list);
        s.push('\n');
    }
    if !read_titles.is_empty() {
        s.push_str("Recently read article titles:\n");
        for t in read_titles {
            let t = t.trim();
            if !t.is_empty() {
                s.push_str("- ");
                s.push_str(t);
                s.push('\n');
            }
        }
    }
    s.trim_end().to_string()
}

/// Deterministic fingerprint of the profile (FNV-1a 64-bit → 16 hex). std-only.
pub fn profile_fingerprint(profile: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in profile.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[Uuid]) -> HashSet<Uuid> {
        list.iter().copied().collect()
    }

    #[test]
    fn normalize_score_passes_through_unit_range() {
        assert_eq!(normalize_score(0.0), 0.0);
        assert_eq!(normalize_score(0.5), 0.5);
        assert_eq!(normalize_score(1.0), 1.0);
    }

    #[test]
    fn normalize_score_divides_0_100_scale() {
        assert!((normalize_score(85.0) - 0.85).abs() < 1e-6);
        assert_eq!(normalize_score(100.0), 1.0);
    }

    #[test]
    fn normalize_score_clamps_out_of_range() {
        assert_eq!(normalize_score(-5.0), 0.0);
        assert_eq!(normalize_score(9999.0), 1.0);
        assert_eq!(normalize_score(f32::NAN), 0.0);
        assert_eq!(normalize_score(f32::INFINITY), 0.0);
    }

    #[test]
    fn parse_scores_happy_path() {
        let id = Uuid::new_v4();
        let raw = format!(r#"[{{"id":"{id}","score":85,"reason":"x"}}]"#);
        let r = parse_relevance_scores(&raw, &ids(&[id])).unwrap();
        assert_eq!(r.len(), 1);
        assert!((r[0].score - 0.85).abs() < 1e-6);
    }

    #[test]
    fn parse_scores_strips_prose_and_fences() {
        let id = Uuid::new_v4();
        let raw = format!("Here:\n```json\n[{{\"id\":\"{id}\",\"score\":50}}]\n```\n");
        let r = parse_relevance_scores(&raw, &ids(&[id])).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn parse_scores_drops_unknown_ids() {
        let known = Uuid::new_v4();
        let unknown = Uuid::new_v4();
        let raw = format!(
            r#"[{{"id":"{unknown}","score":90}},{{"id":"not-a-uuid","score":80}},{{"id":"{known}","score":70}}]"#
        );
        let r = parse_relevance_scores(&raw, &ids(&[known])).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].article_id, known);
    }

    #[test]
    fn parse_scores_dedupes_by_id() {
        let id = Uuid::new_v4();
        let raw = format!(r#"[{{"id":"{id}","score":90}},{{"id":"{id}","score":10}}]"#);
        let r = parse_relevance_scores(&raw, &ids(&[id])).unwrap();
        assert_eq!(r.len(), 1);
        assert!((r[0].score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn parse_scores_omits_empty_reason() {
        let id = Uuid::new_v4();
        let raw = format!(r#"[{{"id":"{id}","score":50,"reason":"  "}}]"#);
        let r = parse_relevance_scores(&raw, &ids(&[id])).unwrap();
        assert_eq!(r[0].reasoning, None);
    }

    #[test]
    fn parse_scores_rejects_non_array() {
        assert!(parse_relevance_scores("no array", &ids(&[])).is_err());
    }

    #[test]
    fn build_profile_empty_is_placeholder() {
        assert_eq!(build_profile(&[], &[]), "(no profile yet)");
    }

    #[test]
    fn build_profile_includes_tags_and_titles() {
        let p = build_profile(&[("rust".into(), 14)], &["Hello".into()]);
        assert!(p.contains("rust (x14)"));
        assert!(p.contains("- Hello"));
    }

    #[test]
    fn profile_fingerprint_is_deterministic() {
        assert_eq!(profile_fingerprint("abc"), profile_fingerprint("abc"));
        assert_ne!(profile_fingerprint("abc"), profile_fingerprint("abd"));
        assert_eq!(profile_fingerprint("abc").len(), 16);
    }
}
