use serde::Serialize;
use uuid::Uuid;

/// リポジトリが返す「素の集計行」。週あたり本数は持たず、直近30日の本数だけを持つ。
///
/// feed_id は read model の相関キーなので、あえて feeds スライスの `FeedId` newtype を
/// import せず `Uuid` を直接使う。これはスライス間の型結合を作らないための意図的な選択で、
/// グローバル集計の前例 `stats` が `feeds`/`articles` のキーを一切持たず素の `i64` を返すのと
/// 同じ方針（読み取り read model はドメイン newtype を跨いで持ち込まない）。serde で UUID 文字列に
/// シリアライズされ、フロントの `feed_id: string` と一致する。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FeedOverviewRow {
    pub feed_id: Uuid,
    pub total_count: i64,
    pub unread_count: i64,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub recent_count_30d: i64,
}

/// API レスポンス1行（読み取り専用 read model）。
#[derive(Debug, Clone, Serialize)]
pub struct FeedOverview {
    pub feed_id: Uuid,
    pub total_count: i64,
    pub unread_count: i64,
    /// 最終投稿時刻。記事ゼロ or published_at が全て NULL なら None。
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 週あたり投稿本数（直近30日の本数から換算、小数1桁に丸め済み）。
    pub posts_per_week: f64,
}

/// 投稿頻度の純粋関数。直近30日 = 30/7 週なので per_week = count * 7 / 30。
/// **小数第1位に丸めて返す**（payload を綺麗に保ち、結合テストの値 assert も簡潔にするため）。
/// この丸めは表示用メトリクスとして意図的で、家庭内・単一ユーザ規模では情報欠落は問題にならない。
///
/// 「最終投稿経過日数」は now() 依存を避けるため backend では計算せず、
/// last_published_at をそのまま返してフロントで「N日前」に整形する（土台設計 §3）。
pub fn posts_per_week(recent_count_30d: i64) -> f64 {
    let raw = (recent_count_30d as f64) * 7.0 / 30.0;
    (raw * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn zero_recent_posts_is_zero_per_week() {
        assert!(approx(posts_per_week(0), 0.0));
    }

    #[test]
    fn thirty_in_thirty_days_is_seven_per_week() {
        assert!(approx(posts_per_week(30), 7.0));
    }

    #[test]
    fn fifteen_in_thirty_days_is_three_and_half_per_week() {
        assert!(approx(posts_per_week(15), 3.5));
    }

    #[test]
    fn two_in_thirty_days_rounds_to_point_five() {
        // raw = 0.4666… → 1桁丸めで 0.5。結合テストの feed A が踏むケース。
        assert!(approx(posts_per_week(2), 0.5));
    }

    #[test]
    fn ten_in_thirty_days_rounds_to_two_point_three() {
        // raw = 2.3333… → 2.3。丸めが切り捨て側に倒れることの確認。
        assert!(approx(posts_per_week(10), 2.3));
    }

    #[test]
    fn result_is_non_negative_and_increases_with_count() {
        assert!(posts_per_week(30) > posts_per_week(0));
        assert!(posts_per_week(100) >= 0.0);
    }
}
