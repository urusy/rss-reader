-- 機能利用状況の記録（可視化は /api/usage/summary → フロント /usage）。
--
-- 単一ユーザー・低頻度（多くて数百件/日）なので生イベントを直接集計する。
-- ロールアップ・パーティションは作らない。保持は USAGE_RETENTION_DAYS
-- （既定365日、0で無効）による日次パージのみ。

-- HTTP 由来（track_usage ミドルウェア）+ クライアント申告（POST /api/usage/events）。
CREATE TABLE IF NOT EXISTS usage_events (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- 機能キー: 'summarize','search','tts_play' 等。features/usage/domain.rs の対応表が正典。
    feature     TEXT NOT NULL,
    source      TEXT NOT NULL DEFAULT 'server' CHECK (source IN ('server', 'client')),
    -- HTTP レスポンスステータス（server 由来のみ。client は NULL）。
    -- raw には失敗も残し、集計側で成功（<400）のみを「利用」と数える。
    status      SMALLINT,
    -- クライアント申告の軽い文脈。例 {"source":"summary"}（tts_play）。server は NULL。
    meta        JSONB
);
CREATE INDEX IF NOT EXISTS idx_usage_events_time_feature
    ON usage_events (occurred_at DESC, feature);

-- LLM 実呼び出し（anthropic.rs が response.usage を捕捉）。
-- キャッシュヒットはここに現れない → usage_events の summarize/translate 件数との差 = キャッシュ節約。
CREATE TABLE IF NOT EXISTS llm_usage_events (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    occurred_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- summarize / translate / chat / suggest_tags / digest / score_relevance / cluster_summary
    purpose       TEXT NOT NULL,
    model         TEXT NOT NULL,
    input_tokens  BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_llm_usage_events_occurred
    ON llm_usage_events (occurred_at DESC);
