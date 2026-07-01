-- 機能: 要約/翻訳のモデル・プロンプトを実行時に設定（設定画面から）。
-- singleton 行（id=1）に override を保持する。既存の instapaper_credentials /
-- read_later_settings と同じ単一ユーザー方式。
-- NULL 列 = 「override 無し」= 既定にフォールバック（モデル→ANTHROPIC_MODEL、
-- プロンプト→バックエンド組込みの既定テンプレート）。
CREATE TABLE IF NOT EXISTS llm_settings (
    id               INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    summarize_model  TEXT,
    summarize_prompt TEXT,
    translate_model  TEXT,
    translate_prompt TEXT,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
