import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { api, type Feed, type FeedOverview, type Folder } from "@/lib/api";
import { useApp } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { lastPostLabel, postsPerWeekLabel } from "@/lib/format";

/**
 * フィード管理（/manage）。記事一覧から分離した管理画面。
 * フォルダ CRUD・フィードの改名/フォルダ割当/再取得/削除、未読数・投稿統計の表示。
 * feeds/folders は store 共有リソースを使い、変更は Sidebar ツリーにも即反映する。
 */
export default function FeedManage() {
  const app = useApp();
  const [overview, { refetch: refetchOverview }] = createResource(() =>
    api.listFeedOverview(),
  );
  const [newFolder, setNewFolder] = createSignal("");

  const overviewById = createMemo(
    () =>
      new Map<string, FeedOverview>(
        (overview() ?? []).map((o) => [o.feed_id, o]),
      ),
  );

  const refetchAll = async () => {
    app.refetchFeeds();
    app.refetchFolders();
    await refetchOverview();
  };

  const createFolder = async () => {
    const name = newFolder().trim();
    if (!name) return;
    try {
      await api.createFolder(name);
      setNewFolder("");
      app.refetchFolders();
    } catch (e) {
      alert(`フォルダ作成に失敗: ${String(e)}`);
    }
  };

  const renameFolder = async (f: Folder) => {
    const name = prompt("新しいフォルダ名", f.name);
    if (name == null || !name.trim()) return;
    try {
      await api.updateFolder(f.id, name.trim());
      app.refetchFolders();
    } catch (e) {
      alert(`改名に失敗: ${String(e)}`);
    }
  };

  const deleteFolder = async (f: Folder) => {
    if (!confirm(`フォルダ「${f.name}」を削除しますか？（配下フィードは未分類になります）`))
      return;
    try {
      await api.deleteFolder(f.id);
      await refetchAll();
    } catch (e) {
      alert(`削除に失敗: ${String(e)}`);
    }
  };

  const renameFeed = async (feed: Feed) => {
    const title = prompt("新しいタイトル", feed.title ?? "");
    if (title == null || !title.trim()) return;
    try {
      await api.updateFeed(feed.id, { title: title.trim() });
      app.refetchFeeds();
    } catch (e) {
      alert(`改名に失敗: ${String(e)}`);
    }
  };

  const assignFolder = async (feed: Feed, value: string) => {
    try {
      await api.updateFeed(feed.id, { folder_id: value === "" ? null : value });
      app.refetchFeeds();
    } catch (e) {
      alert(`フォルダ変更に失敗: ${String(e)}`);
    }
  };

  const refreshFeed = async (feed: Feed) => {
    try {
      await api.refreshFeed(feed.id);
      await refetchAll();
    } catch (e) {
      alert(`再取得に失敗: ${String(e)}`);
    }
  };

  const deleteFeed = async (feed: Feed) => {
    if (!confirm(`フィード「${feed.title ?? feed.url}」を削除しますか？`)) return;
    try {
      await api.deleteFeed(feed.id);
      await refetchAll();
    } catch (e) {
      alert(`削除に失敗: ${String(e)}`);
    }
  };

  return (
    <div class="space-y-6">
      <h1 class="text-2xl font-bold tracking-tight">フィード管理</h1>

      <Card>
        <CardHeader>
          <CardTitle>フォルダ</CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <div class="flex gap-1">
            <Input
              placeholder="新しいフォルダ名"
              value={newFolder()}
              onInput={(e) => setNewFolder(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && createFolder()}
            />
            <Button size="sm" onClick={createFolder}>
              作成
            </Button>
          </div>
          <ul class="divide-y divide-border">
            <For
              each={app.folders()}
              fallback={
                <p class="text-sm text-muted-foreground">フォルダがありません。</p>
              }
            >
              {(f) => (
                <li class="flex items-center justify-between gap-2 py-2">
                  <span class="text-sm">{f.name}</span>
                  <div class="flex gap-1">
                    <Button size="sm" variant="ghost" onClick={() => renameFolder(f)}>
                      改名
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => deleteFolder(f)}>
                      削除
                    </Button>
                  </div>
                </li>
              )}
            </For>
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>フィード</CardTitle>
        </CardHeader>
        <CardContent>
          <ul class="divide-y divide-border">
            <For
              each={app.feeds()}
              fallback={
                <p class="text-sm text-muted-foreground">フィードがありません。</p>
              }
            >
              {(feed) => {
                const o = () => overviewById().get(feed.id);
                return (
                  <li class="space-y-1 py-3">
                    <div class="flex items-center justify-between gap-2">
                      <span class="min-w-0 truncate text-sm font-medium">
                        {feed.title ?? feed.url}
                      </span>
                      <Show when={(o()?.unread_count ?? 0) > 0}>
                        <Badge variant="unread">未読 {o()?.unread_count}</Badge>
                      </Show>
                    </div>
                    <p class="truncate text-xs text-muted-foreground">{feed.url}</p>
                    <p class="text-xs text-muted-foreground">
                      総 {o()?.total_count ?? 0} 件 ・{" "}
                      {lastPostLabel(o()?.last_published_at ?? null)} ・{" "}
                      {postsPerWeekLabel(o()?.posts_per_week ?? 0)}
                    </p>
                    <div class="flex flex-wrap items-center gap-1">
                      <select
                        class="h-8 rounded-md border border-input bg-background px-2 text-xs"
                        value={feed.folder_id ?? ""}
                        onChange={(e) => assignFolder(feed, e.currentTarget.value)}
                      >
                        <option value="">未分類</option>
                        <For each={app.folders()}>
                          {(fl) => <option value={fl.id}>{fl.name}</option>}
                        </For>
                      </select>
                      <Button size="sm" variant="ghost" onClick={() => renameFeed(feed)}>
                        改名
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => refreshFeed(feed)}>
                        再取得
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => deleteFeed(feed)}>
                        削除
                      </Button>
                    </div>
                  </li>
                );
              }}
            </For>
          </ul>
        </CardContent>
      </Card>
    </div>
  );
}
