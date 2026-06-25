// Thin typed client over the Rust backend. All paths are proxied to :8080 in dev.

export interface Feed {
  id: string;
  url: string;
  title: string | null;
  created_at: string;
  last_fetched_at: string | null;
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

  listArticles: (params?: { feed_id?: string; unread?: boolean }) => {
    const q = new URLSearchParams();
    if (params?.feed_id) q.set("feed_id", params.feed_id);
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
