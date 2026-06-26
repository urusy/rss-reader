use serde::Serialize;

/// 購読状況のスナップショット（読み取り専用の集計結果）。
#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub feeds: i64,
    pub articles: i64,
    pub unread: i64,
}
