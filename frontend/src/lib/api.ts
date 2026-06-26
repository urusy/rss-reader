// Thin typed client over the Rust backend. All paths are proxied to :8080 in dev.

export interface Feed {
  id: string;
  url: string;
  title: string | null;
  folder_id: string | null;
  created_at: string;
  last_fetched_at: string | null;
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

async function http<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
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
  deleteFeed: (id: string) => http<void>(`/api/feeds/${id}`, { method: "DELETE" }),
  listFeedOverview: () => http<FeedOverview[]>("/api/feeds/overview"),

  /**
   * フィードの部分更新（リネーム / フォルダ割当 / 未分類化）。
   * double-option セマンティクス:
   *   - キーを渡さない       => その項目は据え置き
   *   - folder_id: "<uuid>" => そのフォルダへ割当
   *   - folder_id: null     => 未分類化（割当解除）
   * 解除は必ず `null` を渡す（undefined はキーが出ず据え置きになる）。
   */
  updateFeed: (id: string, patch: { title?: string; folder_id?: string | null }) =>
    http<Feed>(`/api/feeds/${id}`, { method: "PATCH", body: JSON.stringify(patch) }),

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

  getInstapaperStatus: () => http<InstapaperStatus>("/api/instapaper/status"),
  saveInstapaperCredentials: (creds: { username: string; password: string }) =>
    http<InstapaperStatus>("/api/instapaper/credentials", {
      method: "PUT",
      body: JSON.stringify(creds),
    }),
  deleteInstapaperCredentials: () =>
    http<void>("/api/instapaper/credentials", { method: "DELETE" }),
  // 05 が所有する POST /api/read-later の呼び口。記事 id を取る（生 URL は取らない）。
  saveToReadLater: (articleId: string) =>
    http<void>("/api/read-later", {
      method: "POST",
      body: JSON.stringify({ article_id: articleId }),
    }),
  summarize: (id: string, lang = "ja") =>
    http<Article>(`/api/articles/${id}/summarize`, {
      method: "POST",
      body: JSON.stringify({ lang }),
    }),
  translate: (id: string, lang = "ja") =>
    http<Article>(`/api/articles/${id}/translate`, {
      method: "POST",
      body: JSON.stringify({ lang }),
    }),
};
