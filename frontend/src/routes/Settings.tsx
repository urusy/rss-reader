import { createResource, createSignal, For, Show } from "solid-js";
import { api, errorStatus, type ImportSummary } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { useApp } from "@/lib/store";

export default function Settings() {
  const [status, { refetch }] = createResource(() => api.getInstapaperStatus());
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.saveInstapaperCredentials({
        username: username().trim(),
        password: password(),
      });
      setUsername("");
      setPassword("");
      await refetch();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.deleteInstapaperCredentials();
      await refetch();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // --- OPML (#17) ---
  const [opmlBusy, setOpmlBusy] = createSignal(false);
  const [opmlMsg, setOpmlMsg] = createSignal<string | null>(null);
  const [opmlError, setOpmlError] = createSignal<string | null>(null);

  const exportOpml = async () => {
    setOpmlBusy(true);
    setOpmlError(null);
    try {
      const blob = await api.exportOpml();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "feeds.opml";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setOpmlError(String(e));
    } finally {
      setOpmlBusy(false);
    }
  };

  const importOpml = async (file: File) => {
    setOpmlBusy(true);
    setOpmlError(null);
    setOpmlMsg(null);
    try {
      const r = await api.importOpml(await file.text());
      setOpmlMsg(
        `フィード ${r.imported_feeds} 件 / フォルダ ${r.imported_folders} 件 / スキップ ${r.skipped} 件`,
      );
      app.refetchFeeds();
      app.refetchFolders();
    } catch (e) {
      setOpmlError(
        errorStatus(e) === 400 ? "OPML を解析できませんでした" : `インポート失敗: ${String(e)}`,
      );
    } finally {
      setOpmlBusy(false);
    }
  };

  // --- Read-on-Save (#16) ---
  const [rlSettings, { mutate: mutateRl }] = createResource(() =>
    api.getReadLaterSettings(),
  );
  const onToggleReadOnSave = async (next: boolean) => {
    const prev = rlSettings();
    mutateRl({ mark_read_on_save: next });
    try {
      mutateRl(await api.setReadLaterSettings(next));
    } catch (e) {
      mutateRl(prev);
      alert(`設定の更新に失敗しました: ${String(e)}`);
    }
  };

  // --- バックアップ / 復元 ---
  const app = useApp();
  const [bkToken, setBkToken] = createSignal(
    typeof localStorage !== "undefined"
      ? (localStorage.getItem("backupToken") ?? "")
      : "",
  );
  const onBkToken = (v: string) => {
    setBkToken(v);
    if (typeof localStorage !== "undefined") localStorage.setItem("backupToken", v);
  };
  const [bkBusy, setBkBusy] = createSignal(false);
  const [bkError, setBkError] = createSignal<string | null>(null);
  const [bkResult, setBkResult] = createSignal<ImportSummary | null>(null);
  const [runs, { refetch: refetchRuns }] = createResource(
    () => (bkToken() ? bkToken() : undefined),
    (t) => api.listBackupRuns(t).catch(() => [] as never[]),
  );

  const doExport = async () => {
    setBkBusy(true);
    setBkError(null);
    setBkResult(null);
    try {
      const blob = await api.exportBackup(bkToken());
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "rss-backup.ndjson";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setBkError(String(e));
    } finally {
      setBkBusy(false);
    }
  };

  const doImport = async (file: File) => {
    setBkBusy(true);
    setBkError(null);
    setBkResult(null);
    try {
      const text = await file.text();
      const summary = await api.importBackup(bkToken(), text);
      setBkResult(summary);
      app.refetchFeeds();
      app.refetchFolders();
      await refetchRuns();
    } catch (e) {
      setBkError(String(e));
    } finally {
      setBkBusy(false);
    }
  };

  return (
    <div class="mx-auto max-w-3xl space-y-6 px-4 py-6">
      <h1 class="text-2xl font-bold tracking-tight">設定</h1>

      <Card>
        <CardHeader>
          <CardTitle>
            <div class="flex items-center gap-2">
              Instapaper 連携
              <Show
                when={status()?.configured}
                fallback={<Badge>未接続</Badge>}
              >
                <Badge variant="unread">接続済み</Badge>
              </Show>
            </div>
          </CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <p class="text-xs text-muted-foreground">
            記事を Instapaper に送るには、Instapaper のメールアドレスとパスワードを登録してください。
          </p>

          <Show when={error()}>
            <p class="text-sm text-destructive">{error()}</p>
          </Show>

          <div class="space-y-2">
            <Input
              type="email"
              placeholder="you@example.com"
              autocomplete="off"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
            />
            <Input
              type="password"
              placeholder="パスワード"
              autocomplete="off"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
            />
          </div>

          <div class="flex gap-2">
            <Button
              onClick={save}
              disabled={busy() || !username().trim() || !password()}
            >
              {busy() ? "保存中…" : "保存"}
            </Button>
            <Show when={status()?.configured}>
              <Button variant="destructive" onClick={remove} disabled={busy()}>
                資格情報を削除
              </Button>
            </Show>
          </div>

          <div class="flex items-start justify-between gap-3 border-t border-border pt-3">
            <div>
              <p class="text-sm">Instapaper に送ったら自動で既読にする</p>
              <p class="text-xs text-muted-foreground">
                「後で読む」に送った記事を未読一覧から外し、未読数の膨張を防ぎます。
              </p>
            </div>
            <Switch
              checked={rlSettings()?.mark_read_on_save ?? false}
              disabled={rlSettings.loading}
              onCheckedChange={(d) => void onToggleReadOnSave(d.checked)}
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>OPML 入出力</CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <p class="text-xs text-muted-foreground">
            他のリーダーからフィードを取り込んだり、購読リストを書き出せます。
            フィードは URL、フォルダは名前で重複排除されます（記事は含まれません）。
          </p>
          <Show when={opmlError()}>
            <p class="text-sm text-destructive">{opmlError()}</p>
          </Show>
          <Show when={opmlMsg()}>
            <p class="text-sm text-muted-foreground">{opmlMsg()}</p>
          </Show>
          <div class="flex flex-wrap items-center gap-2">
            <Button onClick={exportOpml} disabled={opmlBusy()}>
              {opmlBusy() ? "処理中…" : "OPML をエクスポート"}
            </Button>
            <label class="inline-flex">
              <input
                type="file"
                accept=".opml,.xml,text/xml,application/xml"
                class="hidden"
                disabled={opmlBusy()}
                onChange={(e) => {
                  const f = e.currentTarget.files?.[0];
                  if (f) void importOpml(f);
                  e.currentTarget.value = "";
                }}
              />
              <span
                class="inline-flex h-9 cursor-pointer items-center rounded-md border border-input bg-background px-4 text-sm font-medium hover:bg-accent hover:text-accent-foreground pointer-coarse:min-h-11"
                classList={{ "pointer-events-none opacity-50": opmlBusy() }}
              >
                OPML をインポート（ファイル選択）
              </span>
            </label>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>バックアップ / 復元</CardTitle>
        </CardHeader>
        <CardContent class="space-y-3">
          <p class="text-xs text-muted-foreground">
            全データ（記事・フォルダ・既読・要約/翻訳キャッシュ・後で読む）を
            NDJSON で書き出し / 取り込みます。資格情報（Instapaper）は含まれません。
            サーバの BACKUP_TOKEN を入力してください。
          </p>

          <Show when={bkError()}>
            <p class="text-sm text-destructive">{bkError()}</p>
          </Show>
          <Show when={bkResult()}>
            {(r) => (
              <p class="text-sm text-muted-foreground">
                取り込みました: フォルダ {r().folders} / フィード {r().feeds} / 記事{" "}
                {r().articles} / 後で読む {r().read_later} / スキップ {r().skipped} 件
              </p>
            )}
          </Show>

          <Input
            type="password"
            placeholder="BACKUP_TOKEN"
            autocomplete="off"
            value={bkToken()}
            onInput={(e) => onBkToken(e.currentTarget.value)}
          />

          <div class="flex flex-wrap items-center gap-2">
            <Button onClick={doExport} disabled={bkBusy() || !bkToken()}>
              {bkBusy() ? "処理中…" : "エクスポート"}
            </Button>
            <label class="inline-flex">
              <input
                type="file"
                accept=".ndjson,.json,application/x-ndjson"
                class="hidden"
                disabled={bkBusy() || !bkToken()}
                onChange={(e) => {
                  const f = e.currentTarget.files?.[0];
                  if (f) void doImport(f);
                  e.currentTarget.value = "";
                }}
              />
              <span
                class="inline-flex h-9 cursor-pointer items-center rounded-md border border-input bg-background px-4 text-sm font-medium hover:bg-accent hover:text-accent-foreground pointer-coarse:min-h-11"
                classList={{ "pointer-events-none opacity-50": bkBusy() || !bkToken() }}
              >
                インポート（ファイル選択）
              </span>
            </label>
          </div>

          <Show when={(runs()?.length ?? 0) > 0}>
            <div class="space-y-1 pt-2">
              <h3 class="text-xs font-semibold text-muted-foreground">
                pg_dump 実行履歴
              </h3>
              <ul class="space-y-1 text-xs">
                <For each={runs()}>
                  {(run) => (
                    <li class="flex items-center gap-2">
                      <Badge variant={run.status === "succeeded" ? "unread" : undefined}>
                        {run.status}
                      </Badge>
                      <span class="truncate text-muted-foreground">
                        {run.file_path ?? "—"}
                        {run.byte_size != null
                          ? ` (${Math.round(run.byte_size / 1024)} KB)`
                          : ""}
                      </span>
                    </li>
                  )}
                </For>
              </ul>
            </div>
          </Show>
        </CardContent>
      </Card>
    </div>
  );
}
