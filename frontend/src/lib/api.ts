// Thin typed client over the Rust backend. All paths are proxied to :8080 in dev.
// 認証はセッション Cookie（HttpOnly, same-origin）。JS はトークンを一切保持しない。
import { onUnauthorized } from "@/lib/auth";

export interface AuthStatus {
  setup_required: boolean;
  authenticated: boolean;
}

export interface SessionInfo {
  id: string;
  label: string | null;
  created_at: string;
  last_seen_at: string;
  current: boolean;
}

export interface ImportSummary {
  folders: number;
  feeds: number;
  articles: number;
  read_later: number;
  skipped: number;
}

export interface BackupRun {
  id: string;
  started_at: string;
  finished_at: string | null;
  status: "running" | "succeeded" | "failed";
  file_path: string | null;
  byte_size: number | null;
  error: string | null;
}

// backup 用ヘッダ: X-Backup-Token（backup gate）。認証はセッション Cookie が担う。
function backupHeaders(backupToken: string): Record<string, string> {
  return { "X-Backup-Token": backupToken };
}

export interface Feed {
  id: string;
  url: string;
  title: string | null;
  folder_id: string | null;
  created_at: string;
  last_fetched_at: string | null;
  // 通知優先度 (#31): 0=通常 / 1=高。高のフィードの新着のみ Web Push 通知。
  priority: number;
  // クロール時に全文を自動抽出する（ヘッドラインのみのフィード向け）。
  extract_full_content: boolean;
}

export interface Folder {
  id: string;
  name: string;
  position: number;
  created_at: string;
}

export interface Article {
  id: string;
  feed_id: string;
  url: string;
  title: string;
  content: string;
  /** Full body extracted on demand from the source URL; null until extracted. */
  full_content: string | null;
  extracted_at: string | null;
  published_at: string | null;
  is_read: boolean;
  summary: string | null;
  summary_lang: string | null;
  translation: string | null;
  translation_lang: string | null;
  processed_at: string | null;
  created_at: string;
}

export interface FeedOverview {
  feed_id: string;
  total_count: number;
  unread_count: number;
  last_published_at: string | null;
  posts_per_week: number;
}

export interface Stats {
  feeds: number;
  articles: number;
  unread: number;
}

export interface InstapaperStatus {
  configured: boolean;
}

export interface ReadLaterSettings {
  mark_read_on_save: boolean;
}

/** 要約/翻訳のモデル・プロンプト設定（#llm_settings）。
 *  *_model / *_prompt が null のときは default_* にフォールバックする。 */
export interface LlmSettings {
  summarize_model: string | null;
  summarize_prompt: string | null;
  translate_model: string | null;
  translate_prompt: string | null;
  default_model: string;
  default_summarize_prompt: string;
  default_translate_prompt: string;
}

export interface LlmSettingsPatch {
  summarize_model?: string | null;
  summarize_prompt?: string | null;
  translate_model?: string | null;
  translate_prompt?: string | null;
}

export type Combinator = "all" | "any";
export type KeywordTarget = "title" | "content" | "any";
export type DateOp = "older_than_days" | "newer_than_days";
export type Condition =
  | {
      field: "keyword";
      target: KeywordTarget;
      value: string;
      case_sensitive?: boolean;
    }
  | { field: "author"; value: string }
  | { field: "feed"; feed_ids: string[] }
  | { field: "tag"; tag: string }
  | { field: "date"; op: DateOp; days: number };
export type RuleAction =
  | { kind: "mark_read" }
  | { kind: "star" }
  | { kind: "tag"; name: string }
  | { kind: "save" }
  | { kind: "score"; delta: number };
export interface Conditions {
  combinator: Combinator;
  items: Condition[];
}
export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  position: number;
  conditions: Conditions;
  actions: RuleAction[];
  created_at: string;
  updated_at: string;
}
export interface RuleInput {
  name: string;
  enabled?: boolean;
  position?: number;
  conditions: Conditions;
  actions: RuleAction[];
}
export interface RuleTestResult {
  matched_count: number;
  matched_ids: string[];
}

export interface QuerySpec {
  text?: string;
  feed_id?: string;
  folder_id?: string;
  unclassified?: boolean;
  unread_only?: boolean;
  tag_ids?: string[];
}
export interface SavedView {
  id: string;
  name: string;
  query: QuerySpec;
  position: number;
  created_at: string;
}

export interface Cluster {
  id: string;
  title: string;
  size: number;
  summary: string | null;
  summary_lang: string | null;
  created_at: string;
}
export interface ClusterMember {
  cluster_id: string;
  article_id: string;
  title: string;
  url: string;
  feed_id: string;
  feed_title: string | null;
  is_representative: boolean;
  is_duplicate: boolean;
  similarity: number;
}
export interface ClusterWithMembers extends Cluster {
  members: ClusterMember[];
}

export interface RelevanceScore {
  article_id: string;
  score: number; // 0.0 .. 1.0
  reasoning: string | null;
  scored_at: string;
}
export interface ScoreResult {
  scored_count: number;
  profile_hash: string;
  scores: RelevanceScore[];
}
export interface RelevanceProfile {
  profile: string;
  hash: string;
  tag_count: number;
  read_count: number;
}

export interface Digest {
  date: string;
  markdown: string;
  model: string;
  article_count: number;
  created_at: string;
}

export interface Tag {
  id: string;
  name: string;
  color: string | null;
  source: "user" | "ai";
  created_at: string;
  article_count?: number;
}
export interface ArticleTag {
  id: string;
  name: string;
  color: string | null;
  attached_source: "user" | "ai";
  confidence: number | null;
}
export interface TagSuggestion {
  name: string;
  confidence: number | null;
}

export interface AskMessage {
  role: "user" | "assistant";
  content: string;
}
export interface AskResponse {
  answer: string;
}
export interface NotesResponse {
  messages: AskMessage[];
}

export interface Highlight {
  id: string;
  article_id: string;
  quote: string;
  note: string | null;
  start_offset: number | null;
  end_offset: number | null;
  color: string | null;
  created_at: string;
  updated_at: string;
}

export interface NewHighlight {
  quote: string;
  note?: string;
  start_offset?: number;
  end_offset?: number;
  color?: string;
}

export interface FeedHealth {
  feed_id: string;
  last_fetch_status: "ok" | "error" | null;
  last_error: string | null;
  consecutive_failures: number;
  last_fetch_attempted_at: string | null;
  last_fetched_at: string | null;
  last_published_at: string | null;
  health: "healthy" | "stale" | "dead";
}

export interface DiscoveredFeed {
  url: string;
  title: string | null;
  kind: "rss" | "atom" | "json" | "unknown";
  already_subscribed: boolean;
}

export interface MuteRule {
  id: string;
  field: "title" | "content" | "url";
  pattern: string;
  match_type: "contains";
  action: "hide" | "mark_read";
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface MuteApplyReport {
  rules_evaluated: number;
  hidden: number;
  marked_read: number;
}

export interface ImportOpmlResult {
  imported_feeds: number;
  imported_folders: number;
  skipped: number;
}

// エクスポートはファイルダウンロードなので http<T>(JSON前提) を通さずアンカーで開く。
export const OPML_EXPORT_URL = "/api/opml/export";

export interface ReadLaterItem {
  article_id: string;
  status: "pending" | "added" | "failed";
  instapaper_added_at: string | null;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

// --- 利用状況の記録・可視化 ---

/** 期間×機能の時系列1点（GET /api/usage/summary の buckets 要素）。 */
export interface UsageBucket {
  /** バケット先頭の時刻（ISO 8601、date_trunc の結果） */
  bucket: string;
  feature: string;
  count: number;
}

/** LLM 実呼び出しの purpose×model 集計行。キャッシュヒットは含まれない。 */
export interface LlmUsageRow {
  purpose: string;
  model: string;
  calls: number;
  input_tokens: number;
  output_tokens: number;
}

/** tts_play の読み上げ対象内訳（meta.source 別）。 */
export interface TtsSourceRow {
  source: string;
  count: number;
}

export interface UsageSummary {
  buckets: UsageBucket[];
  llm: LlmUsageRow[];
  tts_sources: TtsSourceRow[];
}

// --- 同期（Google Reader 互換 API・機能29） ---

/** ClientLogin で発行された同期トークン（GET /api/sync/tokens の要素）。 */
export interface SyncTokenInfo {
  id: string;
  /** クライアント申告のユーザー名（識別ラベル）。無指定なら null。 */
  label: string | null;
  created_at: string;
  /** 最終利用時刻。一度も同期していなければ null。 */
  last_used_at: string | null;
}

// http<T> は Error(`${status} ${statusText}: ${body}`) を投げる。先頭の status を取り出す。
export function errorStatus(e: unknown): number | null {
  const msg = e instanceof Error ? e.message : String(e);
  const m = /^(\d{3})\b/.exec(msg);
  return m ? Number(m[1]) : null;
}

async function http<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    // セッション Cookie を明示（same-origin 既定と同じだが契約として固定する）。
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      // init.headers を最後に展開し上書き可能に保つ。
      ...(init?.headers ?? {}),
    },
  });
  if (res.status === 401) {
    onUnauthorized(); // セッション失効 → ゲート表示へ（authState signal が反応）
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${text}`);
  }
  // 204 No Content
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export const api = {
  listFeeds: () => http<Feed[]>("/api/feeds"),
  addFeed: (url: string) =>
    http<Feed>("/api/feeds", { method: "POST", body: JSON.stringify({ url }) }),
  // サイト/記事 URL からフィード候補を検出（購読はしない。選択後 addFeed を呼ぶ）。#20
  discoverFeeds: (url: string) =>
    http<{ candidates: DiscoveredFeed[] }>("/api/feeds/discover", {
      method: "POST",
      body: JSON.stringify({ url }),
    }),
  deleteFeed: (id: string) => http<void>(`/api/feeds/${id}`, { method: "DELETE" }),
  listFeedOverview: () => http<FeedOverview[]>("/api/feeds/overview"),
  listFeedHealth: () => http<FeedHealth[]>("/api/feeds/health"),

  /**
   * フィードの部分更新（リネーム / フォルダ割当 / 未分類化）。
   * double-option セマンティクス:
   *   - キーを渡さない       => その項目は据え置き
   *   - folder_id: "<uuid>" => そのフォルダへ割当
   *   - folder_id: null     => 未分類化（割当解除）
   * 解除は必ず `null` を渡す（undefined はキーが出ず据え置きになる）。
   */
  updateFeed: (
    id: string,
    patch: {
      title?: string;
      folder_id?: string | null;
      priority?: number;
      extract_full_content?: boolean;
    },
  ) => http<Feed>(`/api/feeds/${id}`, { method: "PATCH", body: JSON.stringify(patch) }),
  // 通知優先度の更新 (#31): 0=通常 / 1=高。
  setFeedPriority: (id: string, priority: number) =>
    http<Feed>(`/api/feeds/${id}`, {
      method: "PATCH",
      body: JSON.stringify({ priority }),
    }),
  // クロール時の全文自動抽出の切替（ヘッドラインのみのフィード向け）。
  setFeedExtractFullContent: (id: string, extract_full_content: boolean) =>
    http<Feed>(`/api/feeds/${id}`, {
      method: "PATCH",
      body: JSON.stringify({ extract_full_content }),
    }),
  // 当該フィードのみ再取得し、更新後 Feed を返す。
  refreshFeed: (id: string) =>
    http<Feed>(`/api/feeds/${id}/refresh`, { method: "POST" }),

  // --- Web Push 通知 (#31) ---
  // VAPID 公開鍵（SW 購読時の applicationServerKey）。未設定なら 503。
  getPushPublicKey: () => http<{ public_key: string }>("/api/push/public-key"),
  // 購読を登録（endpoint で upsert）。body は PushSubscription.toJSON() 形。
  subscribePush: (subscription: PushSubscriptionJSON) =>
    http<void>("/api/push/subscribe", {
      method: "POST",
      body: JSON.stringify(subscription),
    }),
  // endpoint 指定で購読解除。
  unsubscribePush: (endpoint: string) =>
    http<void>("/api/push/unsubscribe", {
      method: "POST",
      body: JSON.stringify({ endpoint }),
    }),
  // 登録済み購読へテスト通知を送る（疎通確認）。
  testPush: () => http<{ delivered: number }>("/api/push/test", { method: "POST" }),

  listFolders: () => http<Folder[]>("/api/folders"),
  createFolder: (name: string) =>
    http<Folder>("/api/folders", { method: "POST", body: JSON.stringify({ name }) }),
  updateFolder: (id: string, name: string) =>
    http<Folder>(`/api/folders/${id}`, { method: "PATCH", body: JSON.stringify({ name }) }),
  deleteFolder: (id: string) => http<void>(`/api/folders/${id}`, { method: "DELETE" }),

  listArticles: (params?: {
    feed_id?: string;
    folder_id?: string;
    unclassified?: boolean;
    unread?: boolean;
  }) => {
    const q = new URLSearchParams();
    if (params?.feed_id) q.set("feed_id", params.feed_id);
    if (params?.folder_id) q.set("folder_id", params.folder_id);
    if (params?.unclassified) q.set("unclassified", "true");
    if (params?.unread) q.set("unread", "true");
    const qs = q.toString();
    return http<Article[]>(`/api/articles${qs ? `?${qs}` : ""}`);
  },
  // 全文検索（title/content の部分一致, pg_trgm）。q は呼び出し側で trim 済みを渡す。
  searchArticles: (q: string, limit?: number) => {
    const params = new URLSearchParams({ q });
    if (limit != null) params.set("limit", String(limit));
    return http<Article[]>(`/api/search?${params.toString()}`);
  },
  getArticle: (id: string) => http<Article>(`/api/articles/${id}`),
  markRead: (id: string, read = true) =>
    http<void>(`/api/articles/${id}/read`, {
      method: "POST",
      body: JSON.stringify({ read }),
    }),
  // 一括既読。feed_id 省略 = 全体。送信先は /api/articles/read-all（mark-read ではない）。
  markAllRead: (params?: { feed_id?: string }) =>
    http<void>("/api/articles/read-all", {
      method: "POST",
      body: JSON.stringify({ feed_id: params?.feed_id ?? null }),
    }),
  getStats: () => http<Stats>("/api/stats"),

  // --- 後で読む（ローカル保存 = Pocket 風。Instapaper 転送とは別物） ---
  // URL を保存。201 即返しで本文抽出は背景実行（フィード追加と同型）。
  savePage: (url: string) =>
    http<Article>("/api/saved", {
      method: "POST",
      body: JSON.stringify({ url }),
    }),
  // 一覧。state: inbox=マイリスト / archived / all。unread=true で未読のみ。
  listSaved: (params?: {
    state?: "inbox" | "archived" | "all";
    unread?: boolean;
  }) => {
    const q = new URLSearchParams();
    if (params?.state) q.set("state", params.state);
    if (params?.unread) q.set("unread", "true");
    const qs = q.toString();
    return http<Article[]>(`/api/saved${qs ? `?${qs}` : ""}`);
  },
  // アーカイブ（true）/ マイリストへ戻す（false）。
  setSavedArchived: (articleId: string, archived: boolean) =>
    http<void>(`/api/saved/${articleId}`, {
      method: "PATCH",
      body: JSON.stringify({ archived }),
    }),
  // 保存ページの削除（記事ごと消える。RSS 記事のブックマークなら解除のみ）。
  deleteSavedPage: (articleId: string) =>
    http<void>(`/api/saved/${articleId}`, { method: "DELETE" }),

  getInstapaperStatus: () => http<InstapaperStatus>("/api/instapaper/status"),
  saveInstapaperCredentials: (creds: { username: string; password: string }) =>
    http<InstapaperStatus>("/api/instapaper/credentials", {
      method: "PUT",
      body: JSON.stringify(creds),
    }),
  deleteInstapaperCredentials: () =>
    http<void>("/api/instapaper/credentials", { method: "DELETE" }),
  // 記事を Instapaper に保存し、保存状態を返す（冪等）。
  saveForLater: (articleId: string) =>
    http<ReadLaterItem>("/api/read-later", {
      method: "POST",
      body: JSON.stringify({ article_id: articleId }),
    }),
  // 404 のみ「未保存」を意味するので null に畳む。それ以外は再 throw（500等を未保存と誤表示しない）。
  getReadLater: async (articleId: string): Promise<ReadLaterItem | null> => {
    try {
      return await http<ReadLaterItem>(`/api/read-later/${articleId}`);
    } catch (e) {
      if (errorStatus(e) === 404) return null;
      throw e;
    }
  },
  listReadLater: () => http<ReadLaterItem[]>("/api/read-later"),
  // --- ミュート (#19) ---
  listMuteRules: () => http<MuteRule[]>("/api/mute-rules"),
  createMuteRule: (input: {
    field: MuteRule["field"];
    pattern: string;
    action?: MuteRule["action"];
    enabled?: boolean;
  }) =>
    http<MuteRule>("/api/mute-rules", {
      method: "POST",
      body: JSON.stringify(input),
    }),
  updateMuteRule: (
    id: string,
    patch: Partial<Pick<MuteRule, "field" | "pattern" | "action" | "enabled">>,
  ) =>
    http<MuteRule>(`/api/mute-rules/${id}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    }),
  deleteMuteRule: (id: string) =>
    http<void>(`/api/mute-rules/${id}`, { method: "DELETE" }),
  applyMuteRules: () =>
    http<MuteApplyReport>("/api/mute-rules/apply", { method: "POST" }),
  importOpml: (xml: string) =>
    http<ImportOpmlResult>("/api/opml/import", {
      method: "POST",
      headers: { "Content-Type": "application/xml" },
      body: xml,
    }),
  // Cookie 認証なので追加ヘッダ不要（Blob で受けてダウンロードさせる）。
  exportOpml: async (): Promise<Blob> => {
    const res = await fetch(OPML_EXPORT_URL, { credentials: "same-origin" });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    return res.blob();
  },
  getReadLaterSettings: () =>
    http<ReadLaterSettings>("/api/read-later/settings"),
  setReadLaterSettings: (mark_read_on_save: boolean) =>
    http<ReadLaterSettings>("/api/read-later/settings", {
      method: "PUT",
      body: JSON.stringify({ mark_read_on_save }),
    }),
  getLlmSettings: () => http<LlmSettings>("/api/settings/llm"),
  updateLlmSettings: (patch: LlmSettingsPatch) =>
    http<LlmSettings>("/api/settings/llm", {
      method: "PUT",
      body: JSON.stringify(patch),
    }),
  // force=true でキャッシュを無視して再生成（モデル/プロンプト変更後の作り直し）。
  summarize: (id: string, lang = "ja", force = false) =>
    http<Article>(`/api/articles/${id}/summarize`, {
      method: "POST",
      body: JSON.stringify({ lang, force }),
    }),
  translate: (id: string, lang = "ja", force = false) =>
    http<Article>(`/api/articles/${id}/translate`, {
      method: "POST",
      body: JSON.stringify({ lang, force }),
    }),
  // 古い/壊れたキャッシュを破棄（該当カラムを NULL に）。204 を返す。
  deleteSummary: (id: string) =>
    http<void>(`/api/articles/${id}/summarize`, { method: "DELETE" }),
  deleteTranslation: (id: string) =>
    http<void>(`/api/articles/${id}/translate`, { method: "DELETE" }),
  // Rules engine (#28)
  listRules: () => http<Rule[]>("/api/rules"),
  getRule: (id: string) => http<Rule>(`/api/rules/${id}`),
  createRule: (input: RuleInput) =>
    http<Rule>("/api/rules", { method: "POST", body: JSON.stringify(input) }),
  updateRule: (id: string, input: RuleInput) =>
    http<Rule>(`/api/rules/${id}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteRule: (id: string) => http<void>(`/api/rules/${id}`, { method: "DELETE" }),
  testRule: (id: string) =>
    http<RuleTestResult>(`/api/rules/${id}/test`, { method: "POST" }),
  applyRules: () =>
    http<{ processed: number }>("/api/rules/apply", { method: "POST" }),
  // Smart views (#27)
  listSavedViews: () => http<SavedView[]>("/api/saved-views"),
  getSavedView: (id: string) => http<SavedView>(`/api/saved-views/${id}`),
  createSavedView: (body: { name: string; query: QuerySpec; position?: number }) =>
    http<SavedView>("/api/saved-views", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  updateSavedView: (
    id: string,
    body: { name: string; query: QuerySpec; position?: number },
  ) =>
    http<SavedView>(`/api/saved-views/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteSavedView: (id: string) =>
    http<void>(`/api/saved-views/${id}`, { method: "DELETE" }),
  resolveSavedView: (id: string, unread?: boolean) =>
    http<Article[]>(
      `/api/saved-views/${id}/articles${unread ? "?unread=true" : ""}`,
    ),
  // Clustering (#26)
  listClusters: () => http<ClusterWithMembers[]>("/api/clusters"),
  getCluster: (id: string) => http<ClusterWithMembers>(`/api/clusters/${id}`),
  summarizeCluster: (id: string, targetLang?: string) =>
    http<Cluster>(`/api/clusters/${id}/summary`, {
      method: "POST",
      body: JSON.stringify(targetLang ? { target_lang: targetLang } : {}),
    }),
  reclusterNow: () =>
    http<{ clusters: number }>("/api/clusters/recluster", { method: "POST" }),
  // Relevance (#25)
  listRelevanceScores: () => http<RelevanceScore[]>("/api/relevance/scores"),
  scoreRelevance: (refresh = false) =>
    http<ScoreResult>(`/api/relevance/score${refresh ? "?refresh=true" : ""}`, {
      method: "POST",
    }),
  getRelevanceProfile: () => http<RelevanceProfile>("/api/relevance/profile"),
  // Digest (#23)
  getLatestDigest: () => http<Digest>("/api/digest/latest"),
  getDigest: (date: string) =>
    http<Digest>(`/api/digest?date=${encodeURIComponent(date)}`),
  refreshDigest: () => http<Digest>("/api/digest/refresh", { method: "POST" }),
  // Tags (#24)
  listTags: () => http<Tag[]>("/api/tags"),
  createTag: (body: { name: string; color?: string }) =>
    http<Tag>("/api/tags", { method: "POST", body: JSON.stringify(body) }),
  updateTag: (id: string, body: { name: string; color?: string }) =>
    http<Tag>(`/api/tags/${id}`, { method: "PATCH", body: JSON.stringify(body) }),
  deleteTag: (id: string) => http<void>(`/api/tags/${id}`, { method: "DELETE" }),
  getArticleTags: (articleId: string) =>
    http<ArticleTag[]>(`/api/articles/${articleId}/tags`),
  setArticleTags: (articleId: string, tagIds: string[]) =>
    http<ArticleTag[]>(`/api/articles/${articleId}/tags`, {
      method: "PUT",
      body: JSON.stringify({ tag_ids: tagIds }),
    }),
  detachArticleTag: (articleId: string, tagId: string) =>
    http<void>(`/api/articles/${articleId}/tags/${tagId}`, { method: "DELETE" }),
  suggestTags: (articleId: string, refresh = false) =>
    http<TagSuggestion[]>(
      `/api/articles/${articleId}/suggest-tags${refresh ? "?refresh=true" : ""}`,
      { method: "POST" },
    ),
  // Ask Claude (#22): 単一記事への対話 Q&A。messages は user で始まり user で終わる。
  askArticle: (id: string, messages: AskMessage[], save = false) =>
    http<AskResponse>(`/api/articles/${id}/ask`, {
      method: "POST",
      body: JSON.stringify({ messages, save }),
    }),
  askArticles: (ids: string[], messages: AskMessage[]) =>
    http<AskResponse>("/api/articles/ask", {
      method: "POST",
      body: JSON.stringify({ ids, messages }),
    }),
  getArticleNotes: (id: string) =>
    http<NotesResponse>(`/api/articles/${id}/notes`),
  // 記事本文をサーバ側で抽出し full_content をキャッシュ。更新後 Article を返す。
  // full_content が null のまま返ったら「抽出できなかった」= 抜粋にフォールバック。
  extractArticle: (id: string, force = false) =>
    http<Article>(`/api/articles/${id}/extract`, {
      method: "POST",
      body: JSON.stringify({ force }),
    }),
  // --- バックアップ / 復元 ---
  // backup token は X-Backup-Token で送る。API 認証はセッション Cookie が担う。
  exportBackup: async (token: string): Promise<Blob> => {
    const res = await fetch("/api/backup/export", {
      credentials: "same-origin",
      headers: backupHeaders(token),
    });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    return res.blob();
  },
  importBackup: async (token: string, ndjson: string): Promise<ImportSummary> => {
    const res = await fetch("/api/backup/import", {
      method: "POST",
      credentials: "same-origin",
      headers: { ...backupHeaders(token), "Content-Type": "application/x-ndjson" },
      body: ndjson,
    });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    return res.json();
  },
  listBackupRuns: async (token: string): Promise<BackupRun[]> => {
    const res = await fetch("/api/backup/runs", {
      credentials: "same-origin",
      headers: backupHeaders(token),
    });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
    return res.json();
  },
  // --- スター + ハイライト / 注釈（#32。ローカル知識ベース・外部同期しない） ---
  // 星付き記事 id 一覧（新しい順）。一覧の「星付きだけ」絞り込みに使う。
  listStars: () => http<string[]>("/api/stars"),
  addStar: (id: string) => http<void>(`/api/articles/${id}/star`, { method: "PUT" }),
  removeStar: (id: string) =>
    http<void>(`/api/articles/${id}/star`, { method: "DELETE" }),
  getHighlights: (id: string) =>
    http<Highlight[]>(`/api/articles/${id}/highlights`),
  createHighlight: (id: string, body: NewHighlight) =>
    http<Highlight>(`/api/articles/${id}/highlights`, {
      method: "POST",
      body: JSON.stringify(body),
    }),
  updateHighlight: (hid: string, body: { note?: string; color?: string }) =>
    http<Highlight>(`/api/highlights/${hid}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteHighlight: (hid: string) =>
    http<void>(`/api/highlights/${hid}`, { method: "DELETE" }),
  // --- 認証（セッション Cookie） ---
  // ゲート判定（公開エンドポイント。Cookie 無しでも 200）。
  getAuthStatus: () => http<AuthStatus>("/api/auth/status"),
  // 初回セットアップ（パスワード設定 + 即ログイン）。設定済みなら 409 を throw。
  setupPassword: (password: string) =>
    http<{ ok: boolean }>("/api/auth/setup", {
      method: "POST",
      body: JSON.stringify({ password }),
    }),
  // ログイン。不一致 401 / バックオフ中 429 を throw。成功で Set-Cookie。
  login: (password: string) =>
    http<{ ok: boolean }>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ password }),
    }),
  logout: () => http<void>("/api/auth/logout", { method: "POST" }),
  changePassword: (current_password: string, new_password: string) =>
    http<void>("/api/auth/password", {
      method: "PUT",
      body: JSON.stringify({ current_password, new_password }),
    }),
  listSessions: () => http<SessionInfo[]>("/api/auth/sessions"),
  revokeSession: (id: string) =>
    http<void>(`/api/auth/sessions/${id}`, { method: "DELETE" }),

  // --- 利用状況の記録・可視化 ---
  // 期間×機能の時系列 + LLM 消費 + 読み上げ内訳を一括取得。
  getUsageSummary: (days: number, bucket: string) =>
    http<UsageSummary>(
      `/api/usage/summary?days=${days}&bucket=${encodeURIComponent(bucket)}`,
    ),
  // クライアント側で完結する機能（読み上げ等）の利用申告。テレメトリなので
  // fire-and-forget（keepalive でページ遷移中でも取りこぼしにくく）。204 応答。
  recordUsage: (feature: string, meta?: Record<string, unknown>) =>
    fetch("/api/usage/events", {
      method: "POST",
      credentials: "same-origin",
      keepalive: true,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ feature, meta }),
    }).then(() => undefined),

  // --- 同期（Google Reader 互換 API・機能29） ---
  // ClientLogin で発行された同期トークンの一覧と失効（設定 →「同期クライアント」）。
  listSyncTokens: () => http<SyncTokenInfo[]>("/api/sync/tokens"),
  revokeSyncToken: (id: string) =>
    http<void>(`/api/sync/tokens/${id}`, { method: "DELETE" }),
};
