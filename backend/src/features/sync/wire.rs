//! GReader のワイヤ形式。クライアント decoder の都合（文字列/数値の使い分け・
//! 必須キー・text/plain 応答・独自ヘッダ）をこの1ファイルに封じ込める。
//! serde 構造体は snapshot テストで JSON 形を固定する（§9.1）。

use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::Response;
use serde::Serialize;

// ---- 入力: multi-key パラメータ -----------------------------------------------

/// query + form body をマージした multi-key パラメータ。
/// `axum::extract::Form` / serde_urlencoded は `i=..&i=..&a=..&r=..` の反復キーを
/// last-wins で潰すため使用禁止。`form_urlencoded::parse` で
/// `Vec<(String, String)>` に展開しマージする（FreshRSS の $_REQUEST 互換）。
pub struct Params(Vec<(String, String)>);

impl Params {
    pub fn from(query: Option<&str>, body: &[u8]) -> Self {
        let mut pairs: Vec<(String, String)> = Vec::new();
        if let Some(q) = query {
            pairs.extend(
                form_urlencoded::parse(q.as_bytes()).map(|(k, v)| (k.into_owned(), v.into_owned())),
            );
        }
        pairs.extend(form_urlencoded::parse(body).map(|(k, v)| (k.into_owned(), v.into_owned())));
        Self(pairs)
    }

    /// 最初の値（単一値パラメータ用。body より query が先勝ち）。
    pub fn first(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// 反復キーの全値（`i=` / `a=` / `r=` / `s=` 用・出現順）。
    pub fn all(&self, key: &str) -> Vec<&str> {
        self.0
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

// ---- 出力 serde 構造体（型レベルで文字列/数値を固定） --------------------------

#[derive(Debug, Serialize)]
pub struct TagList {
    pub tags: Vec<TagEntry>,
}

#[derive(Debug, Serialize)]
pub struct TagEntry {
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SubscriptionList {
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Serialize)]
pub struct Subscription {
    /// "feed/<uuid>"
    pub id: String,
    /// COALESCE(title, url)
    pub title: String,
    /// フォルダなしは []（キー自体は必ず出す）。
    pub categories: Vec<Category>,
    /// ★必ず出す（欠落時 NNW が id から URL を捏造する）。
    pub url: String,
    /// サイト URL は保持していないため feed URL で代用。
    #[serde(rename = "htmlUrl")]
    pub html_url: String,
    // iconUrl は省略（空文字より安全。optional 実証済み）。
}

/// ★id/label とも必須・Option にしない（NNW decoder が両方 non-optional）。
#[derive(Debug, Serialize)]
pub struct Category {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct ItemRefs {
    #[serde(rename = "itemRefs")]
    pub item_refs: Vec<ItemRef>,
    /// 最終ページではキー自体を省略（空 itemRefs + continuation は NNW 無限ループ）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

/// ★符号付き 10 進の文字列。
#[derive(Debug, Serialize)]
pub struct ItemRef {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct StreamEnvelope {
    /// "user/-/state/com.google/reading-list" 等（NNW decoder 必須）。
    pub id: String,
    /// 現在秒・数値（NNW decoder 必須）。
    pub updated: i64,
    pub items: Vec<Item>,
    /// stream/contents 用（items/contents では常に None）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Item {
    /// 長形式 %016x。
    pub id: String,
    /// ★ミリ秒・文字列（Fluent が ot 水位に実使用 → 実値必須）。
    #[serde(rename = "crawlTimeMsec")]
    pub crawl_time_msec: String,
    /// ★マイクロ秒・文字列（Reeder のソートキー）。
    #[serde(rename = "timestampUsec")]
    pub timestamp_usec: String,
    /// ★秒・数値。
    pub published: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Fluent は canonical[0].href を読む。
    pub canonical: Vec<Href>,
    /// NNW は alternate[0].href を読む。type は付けない（FreshRSS compat 同様）。
    pub alternate: Vec<Href>,
    /// reading-list + read? + starred? + label(folder)?
    pub categories: Vec<String>,
    /// streamId は subscription/list の id と厳密一致。
    pub origin: Origin,
    /// NNW/Fluent は summary しか読まない — content と同一 HTML を重複掲載。
    pub summary: Content,
    pub content: Content,
}

#[derive(Debug, Serialize)]
pub struct Href {
    pub href: String,
}

#[derive(Debug, Serialize)]
pub struct Origin {
    #[serde(rename = "streamId")]
    pub stream_id: String,
    pub title: String,
    #[serde(rename = "htmlUrl")]
    pub html_url: String,
}

#[derive(Debug, Serialize)]
pub struct Content {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct UnreadCounts {
    /// ★未読合計（= reading-list 行の count と同値）・数値。
    pub max: i64,
    pub unreadcounts: Vec<UnreadCountEntry>,
}

#[derive(Debug, Serialize)]
pub struct UnreadCountEntry {
    pub id: String,
    pub count: i64,
    /// ★マイクロ秒・文字列。
    #[serde(rename = "newestItemTimestampUsec")]
    pub newest_item_timestamp_usec: String,
}

/// 全値 String（Reeder/ReadKit/Fluent のログイン確認プローブ）。
#[derive(Debug, Serialize)]
pub struct UserInfo {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(rename = "userProfileId")]
    pub user_profile_id: String,
    #[serde(rename = "userEmail")]
    pub user_email: String,
}

impl UserInfo {
    /// 単一ユーザー固定値（§7.3）。
    pub fn single_user() -> Self {
        Self {
            user_id: "1".to_string(),
            user_name: "reader".to_string(),
            user_profile_id: "1".to_string(),
            user_email: "reader".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct QuickAddResult {
    #[serde(rename = "numResults")]
    pub num_results: i64,
    pub query: String,
    /// subscription/list の id と厳密一致（NNW が直後に照合する — 生命線）。
    #[serde(rename = "streamId", skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(rename = "streamName", skip_serializing_if = "Option::is_none")]
    pub stream_name: Option<String>,
}

// ---- レスポンスヘルパ ----------------------------------------------------------

const TEXT_PLAIN_UTF8: &str = "text/plain; charset=UTF-8";

fn plain(status: StatusCode, body: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, TEXT_PLAIN_UTF8)
        .body(Body::from(body.to_string()))
        .expect("static response")
}

/// 書き込み成功の唯一の形: 200 text/plain "OK"。
/// JSON / 204 はクライアントが失敗と解釈し永久リトライする。
pub fn ok_plain() -> Response {
    plain(StatusCode::OK, "OK")
}

/// トークン不正/欠落: 401 + 両綴りの Bad-Token ヘッダ（クライアントにより
/// 見るヘッダが違う）。
pub fn unauthorized_sync() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::CONTENT_TYPE, TEXT_PLAIN_UTF8)
        .header("Google-Bad-Token", "true")
        .header("X-Reader-Google-Bad-Token", "true")
        .body(Body::from("Unauthorized"))
        .expect("static response")
}

/// ClientLogin 失敗: 歴史的 Google 形式（403 + Error 行）。
pub fn bad_auth_clientlogin() -> Response {
    plain(StatusCode::FORBIDDEN, "Error=BadAuthentication\n")
}

/// ClientLogin レート制限: 非 200 は一律「認証失敗」扱いされるため 429 で安全。
pub fn rate_limited_clientlogin(retry_after: std::time::Duration) -> Response {
    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header(header::CONTENT_TYPE, TEXT_PLAIN_UTF8)
        .header(
            header::RETRY_AFTER,
            retry_after.as_secs().max(1).to_string(),
        )
        .body(Body::from("Error=BadAuthentication\n"))
        .expect("static response")
}

/// ClientLogin 成功。SID = Auth（同値）、LSID=null はリテラル（Vienna 対策の
/// FreshRSS 前例）。
pub fn client_login_ok(token: &str) -> Response {
    plain(
        StatusCode::OK,
        &format!("SID={token}\nLSID=null\nAuth={token}\n"),
    )
}

/// /reader/api/0/token: 提示された auth トークンをそのまま返す
/// （Miniflux 方式「edit token = auth token」）。
pub fn token_response(token: &str) -> Response {
    plain(StatusCode::OK, &format!("{token}\n"))
}

/// 内部エラー: 詳細は tracing のみ（AppError::Database の JSON 形をワイヤに
/// 漏らさない）。
pub fn internal_error() -> Response {
    plain(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- Params ------------------------------------------------------------

    #[test]
    fn params_preserves_repeated_keys_in_order() {
        let p = Params::from(None, b"i=1&i=2&a=x&i=3&r=y");
        assert_eq!(p.all("i"), vec!["1", "2", "3"]);
        assert_eq!(p.all("a"), vec!["x"]);
        assert_eq!(p.all("r"), vec!["y"]);
        assert_eq!(p.first("i"), Some("1")); // last-wins にならない
    }

    #[test]
    fn params_merges_query_then_body() {
        let p = Params::from(Some("s=from-query&n=5"), b"s=from-body");
        assert_eq!(p.first("s"), Some("from-query"));
        assert_eq!(p.all("s"), vec!["from-query", "from-body"]);
        assert_eq!(p.first("n"), Some("5"));
    }

    #[test]
    fn params_decodes_percent_and_plus() {
        let p = Params::from(None, b"s=user%2F-%2Flabel%2FTech+News&Email=a%40b");
        assert_eq!(p.first("s"), Some("user/-/label/Tech News"));
        assert_eq!(p.first("Email"), Some("a@b"));
    }

    #[test]
    fn params_missing_key_is_none_and_empty() {
        let p = Params::from(None, b"");
        assert_eq!(p.first("x"), None);
        assert!(p.all("x").is_empty());
    }

    // ---- snapshot: subscription/list (§7.5) ---------------------------------

    #[test]
    fn subscription_list_snapshot() {
        let v = SubscriptionList {
            subscriptions: vec![Subscription {
                id: "feed/0197a3c2-0000-0000-0000-000000000000".into(),
                title: "Example Feed".into(),
                categories: vec![Category {
                    id: "user/-/label/Tech".into(),
                    label: "Tech".into(),
                }],
                url: "https://example.org/feed.xml".into(),
                html_url: "https://example.org/feed.xml".into(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"subscriptions":[{
                "id":"feed/0197a3c2-0000-0000-0000-000000000000",
                "title":"Example Feed",
                "categories":[{"id":"user/-/label/Tech","label":"Tech"}],
                "url":"https://example.org/feed.xml",
                "htmlUrl":"https://example.org/feed.xml"
            }]})
        );
    }

    // ---- snapshot: tag/list (§7.4) ------------------------------------------

    #[test]
    fn tag_list_snapshot_starred_has_no_type_key() {
        let v = TagList {
            tags: vec![
                TagEntry {
                    id: "user/-/state/com.google/starred".into(),
                    kind: None,
                },
                TagEntry {
                    id: "user/-/label/Tech".into(),
                    kind: Some("folder".into()),
                },
            ],
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"tags":[
                {"id":"user/-/state/com.google/starred"},
                {"id":"user/-/label/Tech","type":"folder"}
            ]})
        );
    }

    // ---- snapshot: stream/items/ids (§7.6) ----------------------------------

    #[test]
    fn item_refs_continuation_key_is_omitted_when_none() {
        let v = ItemRefs {
            item_refs: vec![ItemRef { id: "1523".into() }],
            continuation: None,
        };
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, json!({"itemRefs":[{"id":"1523"}]}));
        assert!(json.get("continuation").is_none());

        let v = ItemRefs {
            item_refs: vec![ItemRef { id: "1523".into() }],
            continuation: Some("1522".into()),
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"itemRefs":[{"id":"1523"}],"continuation":"1522"})
        );
    }

    // ---- snapshot: item (§7.7) — 文字列/数値・単位の罠 -----------------------

    #[test]
    fn stream_envelope_item_snapshot() {
        let v = StreamEnvelope {
            id: "user/-/state/com.google/reading-list".into(),
            updated: 1_751_856_000,
            items: vec![Item {
                id: "tag:google.com,2005:reader/item/00000000000005f3".into(),
                crawl_time_msec: "1751856000123".into(),
                timestamp_usec: "1751856000123456".into(),
                published: 1_751_850_000,
                title: "Post".into(),
                author: None,
                canonical: vec![Href {
                    href: "https://example.org/post".into(),
                }],
                alternate: vec![Href {
                    href: "https://example.org/post".into(),
                }],
                categories: vec![
                    "user/-/state/com.google/reading-list".into(),
                    "user/-/state/com.google/read".into(),
                ],
                origin: Origin {
                    stream_id: "feed/0197a3c2-0000-0000-0000-000000000000".into(),
                    title: "Example Feed".into(),
                    html_url: "https://example.org/feed.xml".into(),
                },
                summary: Content {
                    content: "<p>body</p>".into(),
                },
                content: Content {
                    content: "<p>body</p>".into(),
                },
            }],
            continuation: None,
        };
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(
            json,
            json!({
                "id":"user/-/state/com.google/reading-list",
                "updated":1751856000,
                "items":[{
                    "id":"tag:google.com,2005:reader/item/00000000000005f3",
                    "crawlTimeMsec":"1751856000123",
                    "timestampUsec":"1751856000123456",
                    "published":1751850000,
                    "title":"Post",
                    "canonical":[{"href":"https://example.org/post"}],
                    "alternate":[{"href":"https://example.org/post"}],
                    "categories":[
                        "user/-/state/com.google/reading-list",
                        "user/-/state/com.google/read"
                    ],
                    "origin":{
                        "streamId":"feed/0197a3c2-0000-0000-0000-000000000000",
                        "title":"Example Feed",
                        "htmlUrl":"https://example.org/feed.xml"
                    },
                    "summary":{"content":"<p>body</p>"},
                    "content":{"content":"<p>body</p>"}
                }]
            })
        );
        // crawlTimeMsec / timestampUsec は文字列、published / updated は数値。
        let item = &json["items"][0];
        assert!(item["crawlTimeMsec"].is_string());
        assert!(item["timestampUsec"].is_string());
        assert!(item["published"].is_number());
        assert!(json["updated"].is_number());
        // author 無しはキー自体が消える。
        assert!(item.get("author").is_none());
    }

    // ---- snapshot: unread-count (§7.15) --------------------------------------

    #[test]
    fn unread_counts_snapshot() {
        let v = UnreadCounts {
            max: 47,
            unreadcounts: vec![UnreadCountEntry {
                id: "user/-/state/com.google/reading-list".into(),
                count: 47,
                newest_item_timestamp_usec: "1751856000123456".into(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"max":47,"unreadcounts":[{
                "id":"user/-/state/com.google/reading-list",
                "count":47,
                "newestItemTimestampUsec":"1751856000123456"
            }]})
        );
    }

    // ---- snapshot: user-info / quickadd ---------------------------------------

    #[test]
    fn user_info_all_values_are_strings() {
        assert_eq!(
            serde_json::to_value(UserInfo::single_user()).unwrap(),
            json!({"userId":"1","userName":"reader","userProfileId":"1","userEmail":"reader"})
        );
    }

    #[test]
    fn quick_add_snapshot() {
        let ok = QuickAddResult {
            num_results: 1,
            query: "https://example.org/feed.xml".into(),
            stream_id: Some("feed/0197a3c2-0000-0000-0000-000000000000".into()),
            stream_name: Some("https://example.org/feed.xml".into()),
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            json!({
                "numResults":1,
                "query":"https://example.org/feed.xml",
                "streamId":"feed/0197a3c2-0000-0000-0000-000000000000",
                "streamName":"https://example.org/feed.xml"
            })
        );
        let fail = QuickAddResult {
            num_results: 0,
            query: "garbage".into(),
            stream_id: None,
            stream_name: None,
        };
        assert_eq!(
            serde_json::to_value(&fail).unwrap(),
            json!({"numResults":0,"query":"garbage"})
        );
    }

    // ---- レスポンスヘルパ ------------------------------------------------------

    async fn body_of(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn ok_plain_is_literal_ok_text_plain() {
        let resp = ok_plain();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            TEXT_PLAIN_UTF8
        );
        assert_eq!(body_of(resp).await, "OK");
    }

    #[tokio::test]
    async fn unauthorized_sync_has_both_bad_token_header_spellings() {
        let resp = unauthorized_sync();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(resp.headers().get("Google-Bad-Token").unwrap(), "true");
        assert_eq!(
            resp.headers().get("X-Reader-Google-Bad-Token").unwrap(),
            "true"
        );
        assert_eq!(body_of(resp).await, "Unauthorized");
    }

    #[tokio::test]
    async fn client_login_responses_match_google_format() {
        let resp = client_login_ok("tok123");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_of(resp).await, "SID=tok123\nLSID=null\nAuth=tok123\n");

        let resp = bad_auth_clientlogin();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_of(resp).await, "Error=BadAuthentication\n");

        let resp = rate_limited_clientlogin(std::time::Duration::from_secs(42));
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(resp.headers().get(header::RETRY_AFTER).unwrap(), "42");
    }

    #[tokio::test]
    async fn token_response_echoes_token_with_newline() {
        let resp = token_response("tok123");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_of(resp).await, "tok123\n");
    }

    #[tokio::test]
    async fn internal_error_is_generic_text() {
        let resp = internal_error();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body_of(resp).await, "Internal Server Error");
    }
}
