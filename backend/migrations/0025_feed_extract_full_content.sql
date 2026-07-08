-- フィード個別の「クロール時に全文を自動取得」フラグ。
-- ヘッドラインのみのフィード（description がタイトルの丸写し等）向けに、
-- 新着取込み時に extraction スライスで本文を抽出して full_content に入れる。
-- 既定 FALSE: 従来挙動（EXTRACT_ON_CRAWL=true のグローバル一括のみ）を変えない。
ALTER TABLE feeds
    ADD COLUMN extract_full_content BOOLEAN NOT NULL DEFAULT FALSE;
