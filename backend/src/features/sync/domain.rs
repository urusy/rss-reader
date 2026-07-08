//! GReader プロトコルの値オブジェクトと純関数層。
//!
//! プロトコルの罠（4 形式の item id・秒/msec/usec の使い分け・continuation・
//! edit-tag 意味論）はすべてここに閉じ込め、DB・Axum・時計なしの単体テストで
//! 固める（docs/design/29-sync-api.md §5.1 / §9.1）。

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// n= 省略時の件数（Google 原典の既定）。
pub const DEFAULT_PAGE_SIZE: i64 = 20;
/// n= の clamp 上限（Miniflux の無制限は OOM ベクタ）。
pub const MAX_PAGE_SIZE: i64 = 1000;
/// i= の clamp。超過分は先頭 1000 件を処理する（400 にしない）。
pub const MAX_ITEMS_PER_REQUEST: usize = 1000;
/// FreshRSS 互換の本文上限（バイト）。
pub const MAX_CONTENT_BYTES: usize = 500_000;

/// GReader item id（articles.short_id）。値域 [1, 2^63) を前提とする newtype。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemId(pub i64);

impl ItemId {
    /// stream/items/ids 用: 符号付き 10 進文字列。
    pub fn short_form(self) -> String {
        self.0.to_string()
    }

    /// items/contents 用: 長形式（ゼロ埋め 16 桁小文字 hex）。
    pub fn long_form(self) -> String {
        format!("tag:google.com,2005:reader/item/{:016x}", self.0 as u64)
    }

    /// 4 形式すべてを受理:
    ///   "tag:google.com,2005:reader/item/00000000148b9369"  (long form padded)
    ///   "tag:google.com,2005:reader/item/2f2"               (NNW: unpadded hex)
    ///   "000000000000048c"                                  (Reeder: bare 16-hex)
    ///   "12345" / "-123"                                    (10 進。負値は Fluent の MSB 再解釈)
    /// hex 枝は u64 でパースして i64 へビット再解釈（`i64::from_str_radix` は
    /// MSB 立ち 16-hex でエラーになるため不可）。最後に正値フィルタ:
    /// 自前 ID は常に正なので、負値・0 は「存在しない ID」として None（バッチを
    /// 失敗させず黙って落とす — クライアントは削除済み記事の stale ID を平気で送る）。
    pub fn parse(s: &str) -> Option<ItemId> {
        const PREFIX: &str = "tag:google.com,2005:reader/item/";
        let v: i64 = if let Some(hex) = s.strip_prefix(PREFIX) {
            u64::from_str_radix(hex, 16).ok()? as i64
        } else if s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            u64::from_str_radix(s, 16).ok()? as i64
        } else {
            s.parse::<i64>().ok()?
        };
        (v > 0).then_some(ItemId(v))
    }
}

/// ストリーム ID。`user/-/` と `user/<任意>/` を等価に受理（user-info が
/// userId="1" を返す以上 `user/1/...` も来る）。出力は常に `user/-/`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamId {
    ReadingList,
    Read,
    KeptUnread,
    Starred,
    Feed(Uuid),
    /// `feed/<http(s)://...>`（subscription/edit の ac=subscribe 入力のみ）。
    FeedUrl(String),
    /// URL デコード済みフォルダ名（ここでは二重デコードしない）。
    Label(String),
    /// broadcast / like / 未知 state → accept-and-OK。
    Ignored(String),
}

impl StreamId {
    pub fn parse(raw: &str) -> StreamId {
        if let Some(rest) = raw.strip_prefix("user/") {
            // user/<userid>/ の userid 部分を読み飛ばす（- / 1 / 任意を等価扱い）。
            let Some(idx) = rest.find('/') else {
                return StreamId::Ignored(raw.to_string());
            };
            let tail = &rest[idx + 1..];
            return match tail {
                "state/com.google/reading-list" => StreamId::ReadingList,
                "state/com.google/read" => StreamId::Read,
                "state/com.google/kept-unread" => StreamId::KeptUnread,
                "state/com.google/starred" => StreamId::Starred,
                _ => {
                    if let Some(name) = tail.strip_prefix("label/") {
                        if name.is_empty() {
                            StreamId::Ignored(raw.to_string())
                        } else {
                            StreamId::Label(name.to_string())
                        }
                    } else {
                        StreamId::Ignored(raw.to_string())
                    }
                }
            };
        }
        if let Some(rest) = raw.strip_prefix("feed/") {
            if let Ok(id) = Uuid::parse_str(rest) {
                return StreamId::Feed(id);
            }
            if rest.starts_with("http://") || rest.starts_with("https://") {
                return StreamId::FeedUrl(rest.to_string());
            }
            return StreamId::Ignored(raw.to_string());
        }
        StreamId::Ignored(raw.to_string())
    }

    /// subscription/list・origin.streamId の出力形（厳密一致が要求される）。
    pub fn feed_output(feed_id: Uuid) -> String {
        format!("feed/{feed_id}")
    }

    /// tag/list・categories の出力形。
    pub fn label_output(name: &str) -> String {
        format!("user/-/label/{name}")
    }
}

// ---- epoch 変換（文字列/数値・単位の罠を一点に集約） ------------------------

/// `published` / envelope `updated` 用: 秒・数値。
pub fn epoch_secs(t: DateTime<Utc>) -> i64 {
    t.timestamp()
}

/// `crawlTimeMsec` 用: ミリ秒・JSON 文字列。
pub fn epoch_msec_str(t: DateTime<Utc>) -> String {
    t.timestamp_millis().to_string()
}

/// `timestampUsec` / `newestItemTimestampUsec` 用: マイクロ秒・JSON 文字列。
pub fn epoch_usec_str(t: DateTime<Utc>) -> String {
    t.timestamp_micros().to_string()
}

/// `ot` / `nt` パラメータ（秒）のパース。
pub fn parse_epoch_secs(s: &str) -> Option<DateTime<Utc>> {
    let v = s.trim().parse::<i64>().ok()?;
    DateTime::from_timestamp(v, 0)
}

/// mark-all-as-read の `ts`: 16 桁以上 → マイクロ秒、未満 → 秒（Miniflux
/// ヒューリスティック。Reeder は usec を送る）。欠落・非数値 → now。
pub fn parse_ts_param(s: Option<&str>) -> DateTime<Utc> {
    let Some(s) = s.map(str::trim).filter(|s| !s.is_empty()) else {
        return Utc::now();
    };
    let Ok(v) = s.parse::<i64>() else {
        return Utc::now();
    };
    let parsed = if s.trim_start_matches('-').len() >= 16 {
        DateTime::from_timestamp_micros(v)
    } else {
        DateTime::from_timestamp(v, 0)
    };
    parsed.unwrap_or_else(Utc::now)
}

// ---- keyset ページング -------------------------------------------------------

/// n+1 件フェッチした rows を受け、(返す n 件, continuation) を返す。
/// n+1 件目が存在した時だけ Some — 空ページ・ちょうど n 件のページに
/// continuation を付けない（NNW の無限ループ防止を構造的に保証）。
pub fn paginate(mut rows: Vec<i64>, n: usize) -> (Vec<i64>, Option<String>) {
    if rows.len() > n {
        rows.truncate(n);
        let cont = rows.last().map(|v| v.to_string());
        (rows, cont)
    } else {
        (rows, None)
    }
}

// ---- edit-tag 意味論 ---------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOp {
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
}

/// a/r の StreamId 列 → 実行すべき操作列（重複は除去・入力順を保持）。
/// a=read → MarkRead / r=read → MarkUnread
/// a=kept-unread → MarkUnread / r=kept-unread → MarkRead（read と冗長ペアで来る）
/// a|r=starred → Star/Unstar
/// Label(_) / Ignored(_) / feed 系 → 何も生成しない（受理して OK）。
pub fn plan_edits(add: &[StreamId], remove: &[StreamId]) -> Vec<EditOp> {
    let mut ops = Vec::new();
    let push = |op: EditOp, ops: &mut Vec<EditOp>| {
        if !ops.contains(&op) {
            ops.push(op);
        }
    };
    for s in add {
        match s {
            StreamId::Read => push(EditOp::MarkRead, &mut ops),
            StreamId::KeptUnread => push(EditOp::MarkUnread, &mut ops),
            StreamId::Starred => push(EditOp::Star, &mut ops),
            _ => {}
        }
    }
    for s in remove {
        match s {
            StreamId::Read => push(EditOp::MarkUnread, &mut ops),
            StreamId::KeptUnread => push(EditOp::MarkRead, &mut ops),
            StreamId::Starred => push(EditOp::Unstar, &mut ops),
            _ => {}
        }
    }
    ops
}

// ---- 同期トークン ------------------------------------------------------------

/// 同期トークン: 32 バイト OS 乱数 → base64url unpadded（43 字。'=' を含まず、
/// naive split するクライアント実装に安全）。auth の SessionToken と同形式だが
/// スライス独立のため自前実装。DB にはハッシュのみ保存する。
pub struct SyncToken(String);

impl SyncToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(Base64UrlUnpadded::encode_string(&bytes))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SyncToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SyncToken(***)")
    }
}

// ---- Authorization ヘッダ ----------------------------------------------------

/// "GoogleLogin auth=<token>" のパース（scheme 厳密一致・`auth` 小文字・
/// 最初の '=' 以降全部をトークンとする）。
pub fn parse_google_login_header(value: &str) -> Option<&str> {
    let rest = value.strip_prefix("GoogleLogin ")?;
    let token = rest.trim_start().strip_prefix("auth=")?;
    (!token.is_empty()).then_some(token)
}

// ---- 本文の打ち切り ----------------------------------------------------------

/// 本文 500KB 切り詰め（UTF-8 の char 境界で切る。バイトスライスは panic
/// するため境界を後退させて探す）。
pub fn truncate_content(html: &str) -> &str {
    if html.len() <= MAX_CONTENT_BYTES {
        return html;
    }
    let mut end = MAX_CONTENT_BYTES;
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    &html[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ItemId::parse — 4 実送信形式 + 頑健性 ----------------------------

    #[test]
    fn item_id_parses_long_form_padded() {
        assert_eq!(
            ItemId::parse("tag:google.com,2005:reader/item/00000000148b9369"),
            Some(ItemId(0x148b9369))
        );
    }

    #[test]
    fn item_id_parses_long_form_unpadded_hex() {
        // NNW は unpadded hex を送る。
        assert_eq!(
            ItemId::parse("tag:google.com,2005:reader/item/2f2"),
            Some(ItemId(0x2f2))
        );
    }

    #[test]
    fn item_id_parses_bare_16_hex() {
        // Reeder は bare 16-hex を送る。
        assert_eq!(ItemId::parse("000000000000048c"), Some(ItemId(0x48c)));
    }

    #[test]
    fn item_id_parses_decimal() {
        assert_eq!(ItemId::parse("12345"), Some(ItemId(12345)));
    }

    #[test]
    fn item_id_msb_set_hex_does_not_error() {
        // MSB 立ち 16-hex: u64 経由でパースし i64 へビット再解釈 → 負値 → None。
        // i64::from_str_radix ならエラーになる入力（頑健性の要）。
        assert_eq!(ItemId::parse("ffffffffffffffff"), None);
    }

    #[test]
    fn item_id_negative_decimal_is_none() {
        // Fluent Reader は MSB 立ちを負の 10 進で送り返す → 存在しない ID として
        // 黙って落とす（バッチを失敗させない）。
        assert_eq!(ItemId::parse("-123"), None);
    }

    #[test]
    fn item_id_zero_and_garbage_are_none() {
        assert_eq!(ItemId::parse("0"), None);
        assert_eq!(ItemId::parse(""), None);
        assert_eq!(ItemId::parse("not-an-id"), None);
        assert_eq!(ItemId::parse("tag:google.com,2005:reader/item/zzzz"), None);
    }

    // ---- long_form 可逆性 --------------------------------------------------

    #[test]
    fn long_form_is_zero_padded_16_lowercase() {
        assert_eq!(
            ItemId(0x5f3).long_form(),
            "tag:google.com,2005:reader/item/00000000000005f3"
        );
    }

    #[test]
    fn long_form_roundtrips_at_boundaries() {
        for v in [1i64, 0x2f2, 1 << 62, i64::MAX] {
            let id = ItemId(v);
            assert_eq!(ItemId::parse(&id.long_form()), Some(id), "v={v}");
            assert_eq!(ItemId::parse(&id.short_form()), Some(id), "v={v}");
        }
    }

    // ---- StreamId::parse ---------------------------------------------------

    #[test]
    fn stream_id_user_dash_and_user_1_are_equivalent() {
        for prefix in ["user/-", "user/1", "user/someone"] {
            assert_eq!(
                StreamId::parse(&format!("{prefix}/state/com.google/reading-list")),
                StreamId::ReadingList,
                "prefix={prefix}"
            );
            assert_eq!(
                StreamId::parse(&format!("{prefix}/state/com.google/read")),
                StreamId::Read
            );
            assert_eq!(
                StreamId::parse(&format!("{prefix}/state/com.google/kept-unread")),
                StreamId::KeptUnread
            );
            assert_eq!(
                StreamId::parse(&format!("{prefix}/state/com.google/starred")),
                StreamId::Starred
            );
        }
    }

    #[test]
    fn stream_id_feed_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(StreamId::parse(&format!("feed/{id}")), StreamId::Feed(id));
    }

    #[test]
    fn stream_id_feed_url() {
        assert_eq!(
            StreamId::parse("feed/https://example.org/feed.xml"),
            StreamId::FeedUrl("https://example.org/feed.xml".to_string())
        );
    }

    #[test]
    fn stream_id_label_keeps_decoded_name_verbatim() {
        // 入力は URL デコード済み前提 — ここで二重デコードしない。
        assert_eq!(
            StreamId::parse("user/-/label/Tech News"),
            StreamId::Label("Tech News".to_string())
        );
        assert_eq!(
            StreamId::parse("user/-/label/日本語"),
            StreamId::Label("日本語".to_string())
        );
        // percent-encode されたまま届いた場合もそのまま保持（デコードは handler 層）。
        assert_eq!(
            StreamId::parse("user/-/label/Tech%20News"),
            StreamId::Label("Tech%20News".to_string())
        );
    }

    #[test]
    fn stream_id_broadcast_and_unknown_are_ignored() {
        assert!(matches!(
            StreamId::parse("user/-/state/com.google/broadcast"),
            StreamId::Ignored(_)
        ));
        assert!(matches!(
            StreamId::parse("user/-/state/com.google/like"),
            StreamId::Ignored(_)
        ));
        assert!(matches!(StreamId::parse("garbage"), StreamId::Ignored(_)));
        assert!(matches!(StreamId::parse(""), StreamId::Ignored(_)));
        assert!(matches!(StreamId::parse("user/-"), StreamId::Ignored(_)));
        assert!(matches!(
            StreamId::parse("user/-/label/"),
            StreamId::Ignored(_)
        ));
    }

    #[test]
    fn stream_id_outputs_are_canonical() {
        let id = Uuid::new_v4();
        assert_eq!(StreamId::feed_output(id), format!("feed/{id}"));
        assert_eq!(StreamId::label_output("Tech"), "user/-/label/Tech");
    }

    // ---- epoch 変換 --------------------------------------------------------

    #[test]
    fn epoch_conversions_use_expected_units() {
        let t = DateTime::from_timestamp(1_751_856_000, 123_456_000).unwrap();
        assert_eq!(epoch_secs(t), 1_751_856_000);
        assert_eq!(epoch_msec_str(t), "1751856000123");
        assert_eq!(epoch_usec_str(t), "1751856000123456");
    }

    #[test]
    fn parse_epoch_secs_accepts_seconds_only() {
        assert_eq!(
            parse_epoch_secs("1751856000"),
            DateTime::from_timestamp(1_751_856_000, 0)
        );
        assert_eq!(parse_epoch_secs("garbage"), None);
        assert_eq!(parse_epoch_secs(""), None);
    }

    #[test]
    fn parse_ts_param_heuristic_secs_vs_usec() {
        // 10 桁 → 秒。
        let secs = parse_ts_param(Some("1751856000"));
        assert_eq!(secs.timestamp(), 1_751_856_000);
        // 16 桁 → マイクロ秒（Reeder）。
        let usec = parse_ts_param(Some("1751856000123456"));
        assert_eq!(usec.timestamp(), 1_751_856_000);
        assert_eq!(usec.timestamp_micros(), 1_751_856_000_123_456);
        // 15 桁境界 → 桁数規約どおり秒として解釈するが chrono の表現範囲外
        // （±26万年）なので now フォールバック（クラッシュしないことが要点）。
        let before = Utc::now();
        let border = parse_ts_param(Some("175185600012345"));
        assert!(border >= before && border <= Utc::now() + chrono::Duration::seconds(5));
    }

    #[test]
    fn parse_ts_param_missing_or_garbage_is_now() {
        let before = Utc::now();
        for input in [None, Some(""), Some("garbage")] {
            let t = parse_ts_param(input);
            assert!(t >= before, "input={input:?}");
            assert!(t <= Utc::now() + chrono::Duration::seconds(5));
        }
    }

    // ---- paginate ----------------------------------------------------------

    #[test]
    fn paginate_exact_n_has_no_continuation() {
        let (rows, cont) = paginate(vec![5, 4], 2);
        assert_eq!(rows, vec![5, 4]);
        assert_eq!(cont, None);
    }

    #[test]
    fn paginate_n_plus_one_yields_continuation_of_last_returned() {
        let (rows, cont) = paginate(vec![5, 4, 3], 2);
        assert_eq!(rows, vec![5, 4]);
        assert_eq!(cont, Some("4".to_string()));
    }

    #[test]
    fn paginate_empty_never_has_continuation() {
        let (rows, cont) = paginate(vec![], 2);
        assert!(rows.is_empty());
        assert_eq!(cont, None);
    }

    #[test]
    fn paginate_fewer_than_n_has_no_continuation() {
        let (rows, cont) = paginate(vec![9], 20);
        assert_eq!(rows, vec![9]);
        assert_eq!(cont, None);
    }

    // ---- plan_edits --------------------------------------------------------

    #[test]
    fn plan_edits_read_semantics() {
        assert_eq!(plan_edits(&[StreamId::Read], &[]), vec![EditOp::MarkRead]);
        assert_eq!(plan_edits(&[], &[StreamId::Read]), vec![EditOp::MarkUnread]);
    }

    #[test]
    fn plan_edits_kept_unread_inverts() {
        assert_eq!(
            plan_edits(&[StreamId::KeptUnread], &[]),
            vec![EditOp::MarkUnread]
        );
        assert_eq!(
            plan_edits(&[], &[StreamId::KeptUnread]),
            vec![EditOp::MarkRead]
        );
    }

    #[test]
    fn plan_edits_redundant_pair_dedupes() {
        // NNW は a=read と r=kept-unread を同時に送る → MarkRead 1 回に潰す。
        assert_eq!(
            plan_edits(&[StreamId::Read], &[StreamId::KeptUnread]),
            vec![EditOp::MarkRead]
        );
    }

    #[test]
    fn plan_edits_star_unstar() {
        assert_eq!(plan_edits(&[StreamId::Starred], &[]), vec![EditOp::Star]);
        assert_eq!(plan_edits(&[], &[StreamId::Starred]), vec![EditOp::Unstar]);
    }

    #[test]
    fn plan_edits_labels_and_ignored_produce_nothing() {
        assert_eq!(
            plan_edits(
                &[
                    StreamId::Label("Tech".into()),
                    StreamId::Ignored("user/-/state/com.google/broadcast".into())
                ],
                &[StreamId::Label("Old".into())]
            ),
            Vec::<EditOp>::new()
        );
    }

    // ---- SyncToken ---------------------------------------------------------

    #[test]
    fn sync_token_is_43_chars_base64url_no_equals() {
        let t = SyncToken::generate();
        assert_eq!(t.as_str().len(), 43);
        assert!(!t.as_str().contains('='));
        assert!(t
            .as_str()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn sync_tokens_are_unique_and_debug_hides_value() {
        let a = SyncToken::generate();
        let b = SyncToken::generate();
        assert_ne!(a.as_str(), b.as_str());
        assert_eq!(format!("{a:?}"), "SyncToken(***)");
    }

    // ---- parse_google_login_header -----------------------------------------

    #[test]
    fn google_login_header_parses_token() {
        assert_eq!(
            parse_google_login_header("GoogleLogin auth=abc123_-XYZ"),
            Some("abc123_-XYZ")
        );
    }

    #[test]
    fn google_login_header_rejects_wrong_scheme_or_case() {
        assert_eq!(parse_google_login_header("Bearer abc"), None);
        assert_eq!(parse_google_login_header("googlelogin auth=abc"), None);
        assert_eq!(parse_google_login_header("GoogleLogin AUTH=abc"), None);
        assert_eq!(parse_google_login_header("GoogleLogin auth="), None);
        assert_eq!(parse_google_login_header(""), None);
    }

    // ---- truncate_content --------------------------------------------------

    #[test]
    fn truncate_content_short_input_is_untouched() {
        assert_eq!(truncate_content("<p>hi</p>"), "<p>hi</p>");
    }

    #[test]
    fn truncate_content_cuts_at_char_boundary_without_panic() {
        // 500KB 境界がマルチバイト文字の途中に落ちる入力。
        let mut s = "a".repeat(MAX_CONTENT_BYTES - 1);
        s.push_str("あああ"); // 3 バイト文字が境界を跨ぐ
        let out = truncate_content(&s);
        assert!(out.len() <= MAX_CONTENT_BYTES);
        assert!(out.is_char_boundary(out.len()));
        // 打ち切り後も有効な UTF-8 のまま（&str である時点で保証されるが明示）。
        assert!(out.starts_with('a'));
    }
}
