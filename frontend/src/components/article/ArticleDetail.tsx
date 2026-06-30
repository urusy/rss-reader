import {
  createEffect,
  createResource,
  createSignal,
  onCleanup,
  Show,
} from "solid-js";
import { api, errorStatus, type Article } from "@/lib/api";
import { sanitizeArticleHtml } from "@/lib/sanitize";
import { useApp } from "@/lib/store";
import {
  DWELL_MS,
  findScrollParent,
  readScrollMetrics,
  scrolledEnough,
} from "@/lib/read-trigger";
import { Button } from "@/components/ui/button";

/**
 * 記事本文の描画（要約/翻訳/後で読む/自動既読）。id を prop で受け、
 * 3ペイン右ペイン（?article 駆動）と単体ページ /articles/:id の両方で共用する。
 */
export default function ArticleDetail(props: { id: string | undefined }) {
  const app = useApp();
  const [article, { mutate }] = createResource(() => props.id, api.getArticle);
  const [busy, setBusy] = createSignal<
    "summarize" | "translate" | "extract" | null
  >(null);
  // 全文取得後に「抜粋/全文」を見比べるためのローカル状態。
  const [showExcerpt, setShowExcerpt] = createSignal(false);
  // 抽出を試みたが本文が薄く取得できなかった時のヒント。
  const [extractMiss, setExtractMiss] = createSignal(false);
  let articleEl: HTMLElement | undefined;

  // 後で読む（Instapaper）の保存状態。null = 未保存。
  const [later, { mutate: mutateLater }] = createResource(
    () => props.id,
    api.getReadLater,
  );
  const [savingLater, setSavingLater] = createSignal(false);

  const saveLater = async () => {
    const id = props.id;
    if (!id) return;
    setSavingLater(true);
    try {
      mutateLater(await api.saveForLater(id));
    } catch (e) {
      if (errorStatus(e) === 503) {
        alert("Instapaper が未設定です。設定画面で資格情報を登録してください。");
      } else {
        // 502 等: サーバは failed 行を残すので再取得して反映
        try {
          mutateLater(await api.getReadLater(id));
        } catch {
          /* ignore */
        }
        alert(`保存に失敗しました: ${String(e)}`);
      }
    } finally {
      setSavingLater(false);
    }
  };

  // 「少し読んだら既読」: 開いた瞬間ではなく、滞在(DWELL_MS) かスクロールのどちらかが
  // 先に成立した時点で一度だけ既読化する。別記事へ切り替えると effect が再実行され、
  // onCleanup でタイマー/リスナを破棄するので、すぐ離れた記事は既読にならない。
  // marked は意図的に非リアクティブ（signal にすると effect 依存に入り二重 POST を招く）。
  let marked: string | undefined;
  createEffect(() => {
    const a = article();
    // a.id !== props.id: 記事切替の読み込み中に前記事の値が一瞬返ってもアームしない。
    if (!a || a.is_read || a.id !== props.id || marked === a.id) return;
    const id = a.id;

    const doMark = () => {
      if (marked === id) return;
      marked = id;
      api
        .markRead(id, true)
        .then(() => mutate((prev) => (prev ? { ...prev, is_read: true } : prev)))
        .catch((e) => console.error("auto mark-read failed", e));
      app.markReadLocal(id); // 一覧ペインのグレーアウトを実既読に追従させる
    };

    const timer = setTimeout(doMark, DWELL_MS);
    const scroller = articleEl ? findScrollParent(articleEl) : window;
    const onScroll = () => {
      if (scrolledEnough(readScrollMetrics(scroller))) doMark();
    };
    scroller.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => {
      clearTimeout(timer);
      scroller.removeEventListener("scroll", onScroll);
    });
  });

  const run = async (kind: "summarize" | "translate") => {
    const id = props.id;
    if (!id) return;
    setBusy(kind);
    try {
      const updated: Article =
        kind === "summarize"
          ? await api.summarize(id, "ja")
          : await api.translate(id, "ja");
      mutate(updated);
    } catch (e) {
      alert(`処理に失敗しました: ${String(e)}`);
    } finally {
      setBusy(null);
    }
  };

  // サーバ側で本文を抽出して full_content をキャッシュ。null のまま返れば抜粋にフォールバック。
  const extract = async () => {
    const id = props.id;
    if (!id) return;
    setBusy("extract");
    setExtractMiss(false);
    try {
      const updated = await api.extractArticle(id);
      mutate(updated);
      if (!updated.full_content) setExtractMiss(true);
      else setShowExcerpt(false);
    } catch (e) {
      alert(`全文の取得に失敗しました: ${String(e)}`);
    } finally {
      setBusy(null);
    }
  };

  // 表示する本文 HTML: 全文があり「抜粋表示」でなければ全文、無ければ content。
  // バックエンドで浄化済みでも多層防御で必ず再サニタイズする（既存方針）。
  const bodyHtml = (a: Article) => {
    const useFull = a.full_content && !showExcerpt();
    return sanitizeArticleHtml(useFull ? a.full_content! : a.content);
  };

  return (
    <Show
      when={article()}
      fallback={<p class="text-muted-foreground text-sm">読み込み中…</p>}
    >
      {(a) => (
        <article class="space-y-4" ref={(el) => (articleEl = el)}>
          <header class="space-y-2">
            <h1 class="text-2xl font-bold tracking-tight">{a().title}</h1>
            <a
              href={a().url}
              target="_blank"
              rel="noreferrer"
              class="text-sm text-muted-foreground underline underline-offset-4"
            >
              元記事を開く ↗
            </a>
          </header>

          <div class="flex gap-2">
            <Button size="sm" onClick={() => run("summarize")} disabled={busy() !== null}>
              {busy() === "summarize" ? "要約中…" : "要約 (Claude)"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => run("translate")}
              disabled={busy() !== null}
            >
              {busy() === "translate" ? "翻訳中…" : "翻訳 (Claude)"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={saveLater}
              disabled={savingLater() || later()?.status === "added"}
            >
              {savingLater()
                ? "保存中…"
                : later()?.status === "added"
                  ? "保存済み ✓"
                  : later()?.status === "failed"
                    ? "再試行"
                    : "後で読む"}
            </Button>
            {/* 全文未取得なら「全文を取得」、取得済みなら抜粋/全文トグル。 */}
            <Show
              when={a().full_content}
              fallback={
                <Button
                  size="sm"
                  variant="outline"
                  onClick={extract}
                  disabled={busy() !== null}
                >
                  {busy() === "extract" ? "取得中…" : "全文を取得"}
                </Button>
              }
            >
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setShowExcerpt((v) => !v)}
              >
                {showExcerpt() ? "全文を表示" : "抜粋を表示"}
              </Button>
            </Show>
          </div>

          <Show when={extractMiss()}>
            <p class="text-xs text-muted-foreground">
              全文を取得できませんでした（抜粋を表示中）。
            </p>
          </Show>

          <Show when={later()?.status === "failed" && later()?.last_error}>
            <p class="text-xs text-muted-foreground">保存に失敗: {later()?.last_error}</p>
          </Show>

          <Show when={a().summary}>
            <section class="rounded-lg border border-border bg-muted/40 p-4">
              <h2 class="text-sm font-semibold mb-1">要約</h2>
              {/* prose は本文/翻訳と基本タイポを揃えるため（要約はテキストノードのため Markdown は描画されない） */}
              <div class="prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap">
                {a().summary}
              </div>
            </section>
          </Show>

          <Show when={a().translation}>
            <section class="rounded-lg border border-border bg-muted/40 p-4">
              <h2 class="text-sm font-semibold mb-1">翻訳</h2>
              <div class="prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap">
                {a().translation}
              </div>
            </section>
          </Show>

          {/* 本文は信頼できない HTML。innerHTML 前に必ず浄化する
              （埋め込み <style> によるレイアウト破壊・XSS 対策）。
              full_content があれば優先表示（抜粋トグル時は content）。 */}
          <div
            class="prose prose-sm dark:prose-invert max-w-none"
            innerHTML={bodyHtml(a())}
          />
        </article>
      )}
    </Show>
  );
}
