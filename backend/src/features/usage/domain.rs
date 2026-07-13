//! 機能キーの対応表と入力検証（すべて純関数）。
//!
//! 「どのエンドポイント = どの機能」の知識はこのファイル1箇所に閉じる。
//! 許可リスト方式: 対応表にない (メソッド, パス) は記録しない（None）。

use axum::http::Method;
use serde::Serialize;

/// クライアント申告 meta の直列化サイズ上限（乱用防止の軽いガード）。
const MAX_META_BYTES: usize = 1024;

/// (メソッド, MatchedPath テンプレート) → 機能キー。
///
/// パスは axum 0.8 のテンプレート構文（`{id}`。`:id` ではない）で、
/// `MatchedPath::as_str()` が返す文字列と完全一致で照合する。
/// 記録しない判断も明示的な設計: 一覧 GET・ポーリング・health・auth・
/// /api/usage/* 自身・記事閲覧（article_view はノイズが多く不採用）は載せない。
pub fn feature_key(method: &Method, matched_path: &str) -> Option<&'static str> {
    match (method.as_str(), matched_path) {
        // 読書
        ("POST", "/api/articles/{id}/read") => Some("mark_read"),
        ("POST", "/api/articles/read-all") => Some("mark_read_all"),
        // AI 要約・翻訳（キャッシュヒット込みの要求回数。実呼び出しは llm_usage_events 側）
        ("POST", "/api/articles/{id}/summarize") => Some("summarize"),
        ("DELETE", "/api/articles/{id}/summarize") => Some("summary_delete"),
        ("POST", "/api/articles/{id}/translate") => Some("translate"),
        ("DELETE", "/api/articles/{id}/translate") => Some("translation_delete"),
        // Ask Claude（単記事・横断は合算）
        ("POST", "/api/articles/{id}/ask") => Some("ask"),
        ("POST", "/api/articles/ask") => Some("ask"),
        // 本文抽出・検索
        ("POST", "/api/articles/{id}/extract") => Some("extract"),
        ("GET", "/api/search") => Some("search"),
        // スター・ハイライト・タグ
        ("PUT", "/api/articles/{id}/star") => Some("star"),
        ("POST", "/api/articles/{id}/highlights") => Some("highlight"),
        ("PUT", "/api/articles/{id}/tags") => Some("tag_assign"),
        ("POST", "/api/articles/{id}/suggest-tags") => Some("tag_suggest"),
        // 後で読む（Instapaper 転送・撤去予定の旧機能）
        ("POST", "/api/read-later") => Some("read_later"),
        // 後で読む（ローカル保存 = saved スライス）。パスのプレースホルダ名は
        // ルート登録テンプレートと一字一句一致させること（ズレると黙って未計測）。
        ("POST", "/api/saved") => Some("saved_save"),
        ("PATCH", "/api/saved/{article_id}") => Some("saved_archive"),
        ("DELETE", "/api/saved/{article_id}") => Some("saved_delete"),
        // トークン保存面（iOS ショートカット / ブラウザ拡張）。public 側だが
        // saved::public_routes がルーター単位で track_usage を layer する。
        ("POST", "/api/save") => Some("saved_capture"),
        // フィード管理
        ("POST", "/api/feeds") => Some("feed_add"),
        ("POST", "/api/feeds/{id}/refresh") => Some("feed_refresh"),
        ("DELETE", "/api/feeds/{id}") => Some("feed_delete"),
        ("POST", "/api/feeds/discover") => Some("feed_discover"),
        ("POST", "/api/opml/import") => Some("opml_import"),
        ("GET", "/api/opml/export") => Some("opml_export"),
        // ダイジェスト・クラスタ・関連度
        ("GET", "/api/digest/latest") => Some("digest_view"),
        ("GET", "/api/digest") => Some("digest_view"),
        ("POST", "/api/digest/refresh") => Some("digest_refresh"),
        ("GET", "/api/clusters") => Some("clusters_view"),
        ("POST", "/api/clusters/recluster") => Some("recluster"),
        ("POST", "/api/clusters/{id}/summary") => Some("cluster_summary_req"),
        ("POST", "/api/relevance/score") => Some("relevance_score"),
        // バックアップ
        ("GET", "/api/backup/export") => Some("backup_export"),
        ("POST", "/api/backup/import") => Some("backup_import"),
        // GReader 互換 API (#29)。外部クライアントの同期を計測対象とする
        // （ユーザー決定 2026-07-07）。sync ルータ側で認証の内側に layer される。
        // ids/unread-count = ポーリング心拍、contents = 本文取得、
        // edit = 既読/スター書き込み、subscribe = 購読変更。
        ("GET", "/reader/api/0/stream/items/ids") => Some("greader_sync"),
        ("GET", "/reader/api/0/unread-count") => Some("greader_sync"),
        ("POST", "/reader/api/0/stream/items/contents") => Some("greader_fetch"),
        ("GET", "/reader/api/0/stream/contents") => Some("greader_fetch"),
        ("GET", "/reader/api/0/stream/contents/{*stream}") => Some("greader_fetch"),
        ("POST", "/reader/api/0/edit-tag") => Some("greader_edit"),
        ("POST", "/reader/api/0/mark-all-as-read") => Some("greader_edit"),
        ("POST", "/reader/api/0/subscription/quickadd") => Some("greader_subscribe"),
        ("POST", "/reader/api/0/subscription/edit") => Some("greader_subscribe"),
        ("POST", "/reader/api/0/rename-tag") => Some("greader_subscribe"),
        ("POST", "/reader/api/0/disable-tag") => Some("greader_subscribe"),
        _ => None,
    }
}

/// クライアント申告で受け付ける機能キー。当面は読み上げのみ。
pub fn client_feature_allowed(feature: &str) -> bool {
    matches!(feature, "tts_play")
}

/// クライアント申告 meta の検証。
///
/// tts_play: `{"source": "content"|"summary"|"translation"}` のみ許可。
/// 未知キー・型違い・サイズ超過は拒否（handler が 400 Validation を返す判断材料）。
pub fn validate_client_meta(feature: &str, meta: &serde_json::Value) -> bool {
    if serde_json::to_string(meta)
        .map(|s| s.len())
        .unwrap_or(usize::MAX)
        > MAX_META_BYTES
    {
        return false;
    }
    match feature {
        "tts_play" => {
            let Some(obj) = meta.as_object() else {
                return false;
            };
            obj.len() == 1
                && matches!(
                    obj.get("source").and_then(|v| v.as_str()),
                    Some("content" | "summary" | "translation")
                )
        }
        _ => false,
    }
}

/// 集計バケット単位の検証。SQL の `date_trunc($1, ..)` へ渡す文字列を
/// この3値に固定する（任意文字列インジェクションの遮断）。
pub fn bucket_unit(s: &str) -> Option<&'static str> {
    match s {
        "day" => Some("day"),
        "week" => Some("week"),
        "month" => Some("month"),
        _ => None,
    }
}

/// 集計対象日数の正規化（1..=730 に clamp、既定 30）。
pub fn clamp_days(days: Option<i32>) -> i32 {
    days.unwrap_or(30).clamp(1, 730)
}

// --- 集計 read model（repository が返し、そのまま JSON になる） ---

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UsageBucketRow {
    pub bucket: chrono::DateTime<chrono::Utc>,
    pub feature: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LlmUsageRow {
    pub purpose: String,
    pub model: String,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// tts_play の読み上げ対象内訳（meta->>'source' 別の件数）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TtsSourceRow {
    pub source: String,
    pub count: i64,
}

/// GET /api/usage/summary のレスポンス全体。
#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub buckets: Vec<UsageBucketRow>,
    pub llm: Vec<LlmUsageRow>,
    pub tts_sources: Vec<TtsSourceRow>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 記録対象ルートの正典表（ドリフト検知）。
    ///
    /// axum の Router からはルート一覧を列挙できないため、この curated table が
    /// 「新しいエンドポイントを足したら usage の対応表も更新する」チェックリストを兼ねる。
    /// 新スライス・新ルートを追加して記録したい場合: domain::feature_key と
    /// この表の両方に1行足すこと。
    const INSTRUMENTED_ROUTES: &[(&str, &str, &str)] = &[
        ("POST", "/api/articles/{id}/read", "mark_read"),
        ("POST", "/api/articles/read-all", "mark_read_all"),
        ("POST", "/api/articles/{id}/summarize", "summarize"),
        ("DELETE", "/api/articles/{id}/summarize", "summary_delete"),
        ("POST", "/api/articles/{id}/translate", "translate"),
        (
            "DELETE",
            "/api/articles/{id}/translate",
            "translation_delete",
        ),
        ("POST", "/api/articles/{id}/ask", "ask"),
        ("POST", "/api/articles/ask", "ask"),
        ("POST", "/api/articles/{id}/extract", "extract"),
        ("GET", "/api/search", "search"),
        ("PUT", "/api/articles/{id}/star", "star"),
        ("POST", "/api/articles/{id}/highlights", "highlight"),
        ("PUT", "/api/articles/{id}/tags", "tag_assign"),
        ("POST", "/api/articles/{id}/suggest-tags", "tag_suggest"),
        ("POST", "/api/read-later", "read_later"),
        ("POST", "/api/saved", "saved_save"),
        ("PATCH", "/api/saved/{article_id}", "saved_archive"),
        ("DELETE", "/api/saved/{article_id}", "saved_delete"),
        ("POST", "/api/save", "saved_capture"),
        ("POST", "/api/feeds", "feed_add"),
        ("POST", "/api/feeds/{id}/refresh", "feed_refresh"),
        ("DELETE", "/api/feeds/{id}", "feed_delete"),
        ("POST", "/api/feeds/discover", "feed_discover"),
        ("POST", "/api/opml/import", "opml_import"),
        ("GET", "/api/opml/export", "opml_export"),
        ("GET", "/api/digest/latest", "digest_view"),
        ("GET", "/api/digest", "digest_view"),
        ("POST", "/api/digest/refresh", "digest_refresh"),
        ("GET", "/api/clusters", "clusters_view"),
        ("POST", "/api/clusters/recluster", "recluster"),
        ("POST", "/api/clusters/{id}/summary", "cluster_summary_req"),
        ("POST", "/api/relevance/score", "relevance_score"),
        ("GET", "/api/backup/export", "backup_export"),
        ("POST", "/api/backup/import", "backup_import"),
        ("GET", "/reader/api/0/stream/items/ids", "greader_sync"),
        ("GET", "/reader/api/0/unread-count", "greader_sync"),
        (
            "POST",
            "/reader/api/0/stream/items/contents",
            "greader_fetch",
        ),
        ("GET", "/reader/api/0/stream/contents", "greader_fetch"),
        (
            "GET",
            "/reader/api/0/stream/contents/{*stream}",
            "greader_fetch",
        ),
        ("POST", "/reader/api/0/edit-tag", "greader_edit"),
        ("POST", "/reader/api/0/mark-all-as-read", "greader_edit"),
        (
            "POST",
            "/reader/api/0/subscription/quickadd",
            "greader_subscribe",
        ),
        (
            "POST",
            "/reader/api/0/subscription/edit",
            "greader_subscribe",
        ),
        ("POST", "/reader/api/0/rename-tag", "greader_subscribe"),
        ("POST", "/reader/api/0/disable-tag", "greader_subscribe"),
    ];

    fn m(s: &str) -> Method {
        s.parse().unwrap()
    }

    #[test]
    fn feature_key_covers_all_instrumented_routes() {
        for (method, path, expected) in INSTRUMENTED_ROUTES {
            assert_eq!(
                feature_key(&m(method), path),
                Some(*expected),
                "route {method} {path} should map to {expected}"
            );
        }
    }

    #[test]
    fn feature_key_ignores_untracked_routes() {
        // 明示的に記録しないと決めたもの（一覧・閲覧・ポーリング・認証・自分自身）。
        let untracked = [
            ("GET", "/api/articles"),
            ("GET", "/api/articles/{id}"), // article_view は不採用（ユーザー決定）
            ("GET", "/api/feeds"),
            ("GET", "/api/health"),
            ("GET", "/api/stats"),
            ("GET", "/api/feeds/overview"),
            ("POST", "/api/auth/login"),
            ("POST", "/api/auth/logout"),
            ("GET", "/api/usage/summary"),
            ("POST", "/api/usage/events"),
            ("GET", "/api/read-later"),
            ("GET", "/api/saved"), // 一覧閲覧は記録しない（GET /api/articles と同方針）
            ("GET", "/api/relevance/scores"),
            ("GET", "/api/saved-views"),
            ("POST", "/api/rules/apply"),
        ];
        for (method, path) in untracked {
            assert_eq!(
                feature_key(&m(method), path),
                None,
                "route {method} {path} must not be tracked"
            );
        }
        // メソッド違いは別ルート（POST summarize は記録、GET summarize は存在しない=None）。
        assert_eq!(
            feature_key(&Method::GET, "/api/articles/{id}/summarize"),
            None
        );
    }

    #[test]
    fn client_allowlist_accepts_only_tts_play() {
        assert!(client_feature_allowed("tts_play"));
        assert!(!client_feature_allowed("summarize")); // サーバー側キーの詐称は拒否
        assert!(!client_feature_allowed("theme_change"));
        assert!(!client_feature_allowed(""));
    }

    #[test]
    fn tts_meta_accepts_known_sources_only() {
        for src in ["content", "summary", "translation"] {
            assert!(
                validate_client_meta("tts_play", &json!({ "source": src })),
                "source={src} should be accepted"
            );
        }
        assert!(!validate_client_meta(
            "tts_play",
            &json!({ "source": "other" })
        ));
        assert!(!validate_client_meta("tts_play", &json!({ "source": 1 })));
        assert!(!validate_client_meta("tts_play", &json!({})));
        assert!(!validate_client_meta(
            "tts_play",
            &json!({ "source": "summary", "extra": true })
        ));
        assert!(!validate_client_meta("tts_play", &json!("summary")));
        // 未許可 feature への meta は常に拒否。
        assert!(!validate_client_meta(
            "summarize",
            &json!({ "source": "summary" })
        ));
    }

    #[test]
    fn tts_meta_rejects_oversized_payload() {
        let big = "x".repeat(MAX_META_BYTES);
        assert!(!validate_client_meta("tts_play", &json!({ "source": big })));
    }

    #[test]
    fn bucket_unit_allows_only_known_values() {
        assert_eq!(bucket_unit("day"), Some("day"));
        assert_eq!(bucket_unit("week"), Some("week"));
        assert_eq!(bucket_unit("month"), Some("month"));
        assert_eq!(bucket_unit("hour"), None);
        assert_eq!(bucket_unit("day; DROP TABLE usage_events"), None);
        assert_eq!(bucket_unit(""), None);
    }

    #[test]
    fn clamp_days_normalizes_range() {
        assert_eq!(clamp_days(None), 30);
        assert_eq!(clamp_days(Some(7)), 7);
        assert_eq!(clamp_days(Some(0)), 1);
        assert_eq!(clamp_days(Some(-5)), 1);
        assert_eq!(clamp_days(Some(100_000)), 730);
    }
}
