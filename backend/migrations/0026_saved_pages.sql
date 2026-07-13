-- Pocket 風「後で読む」（保存ページ）の基盤。
--
-- 方式: articles.feed_id が NOT NULL のため、保存ページ専用の合成フィードを
-- 1 行植えて任意 URL の記事をぶら下げる。これで stars / tags / highlights /
-- ask / 要約・翻訳 / 検索 / 既読が article_id 経由で無改修に効く。
-- 合成フィードはクロール・GReader 同期・フィード一覧から kind で除外する。

-- フィード種別。'saved' = 保存ページ用の合成フィード（クロール・同期対象外）。
ALTER TABLE feeds ADD COLUMN kind TEXT NOT NULL DEFAULT 'rss'
    CHECK (kind IN ('rss', 'saved'));

-- 合成フィード（単一・固定 UUID。Rust 側 saved::domain::SAVED_FEED_ID と一致）。
-- url は http(s) でないため、FeedUrl::parse を通るユーザー入力
-- （POST /api/feeds・OPML import・GReader quickadd）とは構造的に衝突しない。
INSERT INTO feeds (id, url, title, kind)
VALUES ('00000000-0000-0000-0000-000000000501',
        'internal:saved-pages', '保存したページ', 'saved');

-- 保存ページの sidecar 状態（read_later_items と同じ article_id PK 前例）。
-- archived_at NULL = マイリスト(inbox) / 非 NULL = アーカイブ。
CREATE TABLE saved_pages (
    article_id  UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    saved_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at TIMESTAMPTZ
);

CREATE INDEX idx_saved_pages_inbox ON saved_pages (saved_at DESC)
    WHERE archived_at IS NULL;
