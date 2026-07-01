# 33 読み上げ (TTS) リッスンモード

> 読み手向けメモ: これは**設計スタブ（約1ページ）**。着手前に本書を §「詳細化の注記」に従って個別詳細化すること。確認済みの実ファイル: `frontend/src/lib/api.ts`, `frontend/src/lib/store.tsx`, `frontend/src/routes/{Reader,ArticleView}.tsx`, `frontend/src/lib/read-trigger.ts`, `backend/src/shared/llm/{mod,anthropic.rs}`, `backend/src/features/articles/{service,handler,mod}.rs`, `backend/src/features/mod.rs`, `backend/migrations/`（最新 `0005_search.sql`）。

## 1. 概要 / 価値

記事本文を**音声で読み上げ**、ながら聴き（家事・移動・運動中）と視覚アクセシビリティを支える「リッスンモード」。

- **v1 はフロントのみ・無コスト**。ブラウザ標準の Web Speech API (`window.speechSynthesis` + `SpeechSynthesisUtterance`) で、サニタイズ済み本文（`lib/sanitize.ts` の出力）をプレーンテキスト化して読み上げる。バックエンド・DB・外部 API・トークン消費はゼロ。
- **read-on-dwell との相補**。現状の既読化は「滞在/スクロール」起点（`lib/read-trigger.ts`）。リッスンモードでは**読み上げの一定進捗（例: 80% 到達 or 完了）を既読化トリガに加える**ことで、「聴いて消化した」記事も自然に既読になる。
- **任意で v2: Claude 生成の「listen 向けスクリプト」**。本文は耳で聴くと冗長・URL/箇条書きが読みにくい。LLM 境界（`shared/llm`）で「聴き取りやすい要約ナレーション」を生成し、既存の summary キャッシュと**同じ列方針**で `articles` にキャッシュする。オンデマンド・キャッシュ・`NotEnabled` の既存3原則をそのまま踏襲する。

## 2. 想定スライス & テーブル概略

**v1（フロントのみ）— バックエンド変更・マイグレーションなし。**

- 新規 `frontend/src/lib/tts.ts`: `speechSynthesis` の薄いラッパ（再生 / 一時停止 / 再開 / 停止 / 速度・声の選択 / 進捗コールバック）。`SpeechSynthesisUtterance.onboundary` で進捗を拾い既読化フックへ渡す。
- 新規 UI 部品 `frontend/src/components/reader/ListenBar.tsx`: 再生コントロール（自前 Tailwind、a11y 部品不要）。`routes/ArticleView.tsx` に最小差し込み（import 1行 + JSX 1行）。
- 状態: `lib/store.tsx` に再生状態（再生中 article id / 速度 / 選択ボイス）を持つか、`ArticleView` ローカルに閉じるかは詳細化時に決定（グローバル横断再生が要るならストアへ）。設定（速度・ボイス）は `localStorage` 永続で十分。

**v2（任意・Claude スクリプト）— 新スライスは作らず、既存 `articles` スライスを LLM 前例どおり最小拡張。**

- 新マイグレーション `0006_listen_script.sql`（**着手時に最新番号を再確認**。現状最新は `0005`）: `articles` に `listen_script TEXT NULL` / `listen_script_lang TEXT NULL` を追加（`summary`/`translation` と同じ「本文＋_lang」キャッシュ規約）。既存マイグレーションは編集しない。
- `articles/service.rs` に `generate_listen_script(state, id, lang)` を追加（`summarize_article` を雛形に、キャッシュヒット判定 → `LlmClient` 呼び出し → `save_*` 保存）。`shared/llm` に `ListenScriptRequest` を足すか `SummarizeRequest` を流用するかは詳細化時に判断。

## 3. 主要エンドポイント

- **v1: なし**（フロント完結）。
- **v2（任意）**:
  - `POST /api/articles/{id}/listen-script` — リクエスト `{ "lang": "ja" }`（省略時 `ja`）。レスポンス `200` で更新後 `Article`（`listen_script` / `listen_script_lang` 入り）。既存の summarize/translate ハンドラと同型・同パスプレフィックス（`articles/mod.rs` に `.route()` 1行追加）。
  - API キー未設定時は `AppError::NotEnabled`（503）を返す（要約・翻訳と同じ挙動）。
  - キャッシュ済み & 同一 lang の再要求はトークン消費なしでキャッシュを返す。

## 4. 主なリスク / ops 考慮

- **ブラウザ差**: `speechSynthesis` は実装差が大きい。日本語ボイスの有無・品質はOS/ブラウザ依存（特に Linux/一部 Android で日本語ボイス不在）。`getVoices()` が非同期で空配列を返す既知挙動 → `voiceschanged` イベント待ちが必須。iOS Safari はユーザー操作起点でないと発話しない（自動再生制限）。詳細化時に対応マトリクスを作る。
- **長文の打ち切り**: 一部エンジンは1発話の文字数に上限・タイムアウトがある。本文を文単位にチャンク分割して逐次キューイングする（進捗・既読化計算もチャンク基準）。
- **HTML/記号の読み上げ品質**: サニタイズ後でも URL・コード・絵文字が雑音化する。v1 はプレーンテキスト整形（リンクURL除去・連続空白圧縮）で軽減、本質的改善は v2 の Claude スクリプト。
- **既読化との二重トリガ**: read-on-dwell（滞在/スクロール）と読み上げ進捗の両方が既読化を呼びうる。`store.markReadLocal` は冪等だが、サーバ `set_read` の重複POSTを避けるためフロントでガードする（`read-trigger.ts` の既存ガードに合流させる）。
- **コスト/ops（v2のみ）**: LLM 呼び出しはオンデマンド＋キャッシュで既存方針どおり。TTS 音声自体はブラウザ生成なので**音声合成のサーバコストは発生しない**（クラウド TTS は採らない＝ホスティング/鍵管理/従量課金を持ち込まない）。

## 5. 依存（先に必要な機能）

- **ハード依存: なし**。v1 は既存の本文表示（`ArticleView` + `lib/sanitize.ts`）だけで成立。
- **相補（あると望ましい）**:
  - 機能16系の既読化基盤（`lib/read-trigger.ts`、実装済み）に読み上げ進捗トリガを足す形が自然。
  - v2 は `shared/llm` 境界（実装済み）と `articles` の summary キャッシュ規約に乗る。
- **被依存**: なし（このスタブは他機能の土台にはならない）。

## 6. 工数感

- **v1（フロントのみ）: S〜M**。`tts.ts`（ラッパ＋チャンク分割＋進捗）と `ListenBar.tsx`、`ArticleView` への差し込み、既読化フック合流、`tsc`/手動実機確認（iOS Safari 含む）。ブラウザ差吸収が読めない要素。
- **v2（Claude スクリプト・任意）: +S**。マイグレーション1枚 + `articles/service` 1関数 + ハンドラ/ルート1行 + `scripts/test/api-*.sh`。要約機能の写経でほぼ済む。
- 推奨: **v1 を単独で出荷**し、聴き心地に不満が出たら v2 を別チケットで追加。

## 7. 詳細化の注記

本書はスタブ。**実装着手前に本書を個別詳細化**し、CHEATSHEET / 既存設計書（特に `03-feed-stats.md`・`05-instapaper-integration.md` の章立て）に揃えて以下を確定すること:

1. v1/v2 の出荷スコープ分割（v1 単独出荷を既定とする）。
2. 再生状態をストア管理にするか `ArticleView` ローカルにするか（横断再生の要否で決まる）。
3. ブラウザ/OS ボイス対応マトリクスと iOS 自動再生制限への対処、`voiceschanged` ハンドリング。
4. チャンク分割粒度と、読み上げ進捗 → 既読化のしきい値（例 80%）。`read-trigger.ts` への合流方法。
5. v2 採用時: 最新マイグレーション番号の再確認（`0006_…`）、`listen_script(_lang)` カラム、`shared/llm` の request 型を新設するか流用するか、`scripts/test/api-listen-script.sh` の TDD（`NotEnabled` / キャッシュヒット / 生成）。
6. v2 の README.md マイグレーション登録表・依存グラフ・リスク表への追記。
