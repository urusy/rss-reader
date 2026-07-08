-- Feature 29: Google Reader 互換同期 API (GReader)。
-- 設計: docs/design/29-sync-api.md §4

-- (1) GReader 互換の 64bit item id。クライアントは item id を int64 として hex/dec
--     変換するため UUID は使えない。既存行は created_at 順に採番し、
--     「short_id の大小 = クロール時系列」を全行で成立させる
--     （keyset continuation と ot/nt フィルタの整合のため。物理スキャン順の
--     backfill は不可 — GENERATED AS IDENTITY を使わないのはそのため）。
-- 注意: この UPDATE は articles 全行を書き換える。家庭スケール（数万行）では
--       数秒。適用は稼働の谷間に。
ALTER TABLE articles ADD COLUMN short_id BIGINT;

UPDATE articles a SET short_id = t.rn
FROM (SELECT id, row_number() OVER (ORDER BY created_at ASC, id ASC) AS rn
      FROM articles) t
WHERE a.id = t.id;

CREATE SEQUENCE articles_short_id_seq AS BIGINT;
SELECT setval('articles_short_id_seq',
              COALESCE((SELECT max(short_id) FROM articles), 0) + 1, false);

ALTER TABLE articles
    ALTER COLUMN short_id SET DEFAULT nextval('articles_short_id_seq'),
    ALTER COLUMN short_id SET NOT NULL;
ALTER SEQUENCE articles_short_id_seq OWNED BY articles.short_id;

CREATE UNIQUE INDEX idx_articles_short_id ON articles (short_id);

-- (2) GReader クライアント用トークン（恒久・ハッシュのみ保存）。
--     auth_sessions（30日TTL・sliding）とは寿命が違うため相乗りせず別テーブル。
CREATE TABLE sync_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash TEXT NOT NULL UNIQUE,
    label TEXT,                                   -- ClientLogin の Email 値（クライアント識別用）
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);
