-- 監査 LOW: articles.created_at に index が無く、通知の新着射影
-- (created_at 範囲 + ORDER BY created_at) と時系列系クエリが全表走査になる。
CREATE INDEX IF NOT EXISTS idx_articles_created_at ON articles (created_at);
