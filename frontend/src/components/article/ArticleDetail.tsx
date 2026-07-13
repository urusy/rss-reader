import {
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  Show,
} from "solid-js";
import { useNavigate } from "@solidjs/router";
import { api, errorStatus, type Article } from "@/lib/api";
import { sanitizeArticleHtml } from "@/lib/sanitize";
import { useSelection } from "@/lib/selection";
import { useApp } from "@/lib/store";
import {
  DWELL_MS,
  findScrollParent,
  readScrollMetrics,
  scrolledEnough,
} from "@/lib/read-trigger";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleIndicator,
  CollapsibleContent,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogCloseTrigger,
} from "@/components/ui/dialog";
import { Prose } from "@/components/ui/prose";
import { renderMarkdown } from "@/lib/markdown";
import ArticleAsk from "@/components/article/ArticleAsk";
import RegenerateConfirm from "@/components/article/RegenerateConfirm";
import { StarToggle, Highlights } from "@/components/article/Annotations";
import ListenBar, { type ListenSource } from "@/components/article/ListenBar";
import { htmlToPlainText } from "@/lib/tts";
import TagEditor from "@/components/TagEditor";

/**
 * 記事本文の描画（要約/翻訳/後で読む/自動既読）。id を prop で受け、
 * 3ペイン右ペイン（?article 駆動）と単体ページ /articles/:id の両方で共用する。
 */
export default function ArticleDetail(props: { id: string | undefined }) {
  const app = useApp();
  const scope = useSelection();
  const navigate = useNavigate();
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

  // --- 後で読む（ローカル保存）: /saved 配下で開いたときだけ描画するアクション ---
  // 保存状態は Article に載せず scope（URL）で判定する（articles スライス無改変）。
  const savedScope = () => {
    const s = scope();
    return s.kind === "saved" ? s : null;
  };
  const [savedBusy, setSavedBusy] = createSignal(false);
  const backToSavedList = () => {
    // navigate で ?article が消え、store の bump で一覧が再フェッチされる
    app.bumpSavedList();
    navigate(savedScope()?.archived ? "/saved/archive" : "/saved");
  };
  const toggleArchive = async () => {
    const id = props.id;
    const s = savedScope();
    if (!id || !s) return;
    setSavedBusy(true);
    try {
      await api.setSavedArchived(id, !s.archived);
      backToSavedList();
    } catch (e) {
      alert(`更新に失敗しました: ${String(e)}`);
    } finally {
      setSavedBusy(false);
    }
  };
  const deleteSaved = async () => {
    const id = props.id;
    if (!id) return;
    setSavedBusy(true);
    try {
      await api.deleteSavedPage(id);
      backToSavedList();
    } catch (e) {
      alert(`削除に失敗しました: ${String(e)}`);
    } finally {
      setSavedBusy(false);
    }
  };

  // 「少し読んだら既読」: 開いた瞬間ではなく、滞在(DWELL_MS) かスクロールのどちらかが
  // 先に成立した時点で一度だけ既読化する。別記事へ切り替えると effect が再実行され、
  // onCleanup でタイマー/リスナを破棄するので、すぐ離れた記事は既読にならない。
  // marked は意図的に非リアクティブ（signal にすると effect 依存に入り二重 POST を招く）。
  let marked: string | undefined;
  // 滞在/スクロール（下）と読み上げ進捗（#33 ListenBar）から共通で呼ぶ既読化。
  // marked ガードで一度きり。リッスンモードで「聴いて消化」した記事も既読になる。
  const markReadNow = (id: string) => {
    if (marked === id) return;
    marked = id;
    api
      .markRead(id, true)
      .then(() => mutate((prev) => (prev ? { ...prev, is_read: true } : prev)))
      .catch((e) => console.error("auto mark-read failed", e));
    app.markReadLocal(id); // 一覧ペインのグレーアウトを実既読に追従させる
  };

  createEffect(() => {
    const a = article();
    // a.id !== props.id: 記事切替の読み込み中に前記事の値が一瞬返ってもアームしない。
    if (!a || a.is_read || a.id !== props.id || marked === a.id) return;
    const id = a.id;

    const doMark = () => markReadNow(id);

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

  // force=true でキャッシュを無視して再生成（設定でモデル/プロンプトを変えた後の作り直し）。
  const run = async (kind: "summarize" | "translate", force = false) => {
    const id = props.id;
    if (!id) return;
    setBusy(kind);
    try {
      const updated: Article =
        kind === "summarize"
          ? await api.summarize(id, "ja", force)
          : await api.translate(id, "ja", force);
      mutate(updated);
    } catch (e) {
      alert(`処理に失敗しました: ${String(e)}`);
    } finally {
      setBusy(null);
    }
  };

  // 古い/壊れた要約・翻訳のキャッシュを破棄（HTML 混入した旧結果の掃除など）。
  const [clearing, setClearing] = createSignal<"summary" | "translation" | null>(
    null,
  );
  const clear = async (kind: "summary" | "translation") => {
    const id = props.id;
    if (!id) return;
    setClearing(kind);
    try {
      if (kind === "summary") {
        await api.deleteSummary(id);
        mutate((prev) =>
          prev ? { ...prev, summary: null, summary_lang: null } : prev,
        );
      } else {
        await api.deleteTranslation(id);
        mutate((prev) =>
          prev ? { ...prev, translation: null, translation_lang: null } : prev,
        );
      }
    } catch (e) {
      alert(`削除に失敗しました: ${String(e)}`);
    } finally {
      setClearing(null);
    }
  };

  // 要約/翻訳の削除は確認ダイアログを挟む（キャッシュ破棄は取り消せない）。
  // 「削除する」は DialogCloseTrigger なので、閉じつつ onClick で clear を走らせる。
  const DeleteConfirm = (p: {
    kind: "summary" | "translation";
    label: string;
  }) => (
    <Dialog>
      {/* Ark UI の Trigger/CloseTrigger は <button> を描画するので buttonVariants を class で当てる
          （このバージョンは as prop 非対応。asChild render-prop の代わりに直接装飾する）。 */}
      <DialogTrigger
        class={buttonVariants({ size: "sm", variant: "ghost" })}
        disabled={clearing() !== null}
      >
        {clearing() === p.kind ? "削除中…" : "削除"}
      </DialogTrigger>
      <DialogContent>
        <DialogTitle>{p.label}を削除しますか？</DialogTitle>
        <DialogDescription>
          キャッシュされた{p.label}を削除します。この操作は取り消せません（もう一度作るには
          Claude を呼び直します）。
        </DialogDescription>
        <div class="mt-4 flex justify-end gap-2">
          <DialogCloseTrigger
            class={buttonVariants({ size: "sm", variant: "outline" })}
          >
            キャンセル
          </DialogCloseTrigger>
          <DialogCloseTrigger
            class={buttonVariants({ size: "sm", variant: "destructive" })}
            onClick={() => clear(p.kind)}
          >
            削除する
          </DialogCloseTrigger>
        </div>
      </DialogContent>
    </Dialog>
  );

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

  // 本文の平文化は listenSources から複数回・a() の任意変化で評価されるため、
  // DOMParser 再パースを避けて createMemo で 1 回化する。
  const bodyPlain = createMemo(() => {
    const a = article();
    return a ? htmlToPlainText(bodyHtml(a)) : "";
  });

  // 要約・翻訳（Markdown）の HTML 化は表示と読み上げ平文化の両方から参照されるので memo 化。
  const summaryHtml = createMemo(() => renderMarkdown(article()?.summary));
  const translationHtml = createMemo(() =>
    renderMarkdown(article()?.translation),
  );
  const summaryPlain = createMemo(() =>
    article()?.summary ? htmlToPlainText(summaryHtml()) : "",
  );
  const translationPlain = createMemo(() =>
    article()?.translation ? htmlToPlainText(translationHtml()) : "",
  );

  // 読み上げソース（要約/翻訳/本文）。要約・翻訳は Markdown なので、記号（#, **, ```）を
  // 読み上げないよう renderMarkdown→htmlToPlainText で平文化してから渡す（表示は Prose 側で
  // HTML 化）。本文（bodyHtml=sanitized HTML）も平文化する。
  // 並び順は記事内の表示順（要約 → 翻訳 → 本文）に合わせる。既定選択は本文（key=body）。
  const listenSources = (): ListenSource[] => {
    const a = article();
    if (!a) return [];
    return [
      ...(a.summary
        ? [
            {
              key: "summary",
              label: "要約",
              text: summaryPlain(),
              marksRead: false,
            },
          ]
        : []),
      ...(a.translation
        ? [
            {
              key: "translation",
              label: "翻訳",
              text: translationPlain(),
              marksRead: false,
            },
          ]
        : []),
      { key: "body", label: "本文", text: bodyPlain(), marksRead: true },
    ];
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

          <div class="flex flex-wrap gap-2">
            <StarToggle articleId={a().id} />
            {/* 初回生成は即実行。キャッシュがある「再要約/再翻訳」だけ確認を挟む
                （誤タップ1回で Claude を呼び直してトークンを消費しない）。 */}
            <Show
              when={a().summary}
              fallback={
                <Button
                  size="sm"
                  onClick={() => run("summarize")}
                  disabled={busy() !== null}
                >
                  {busy() === "summarize" ? "要約中…" : "要約 (Claude)"}
                </Button>
              }
            >
              <RegenerateConfirm
                label="要約"
                trigger="再要約 (Claude)"
                busyText="要約中…"
                busy={busy() === "summarize"}
                disabled={busy() !== null}
                variant="default"
                onConfirm={() => run("summarize", true)}
              />
            </Show>
            <Show
              when={a().translation}
              fallback={
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => run("translate")}
                  disabled={busy() !== null}
                >
                  {busy() === "translate" ? "翻訳中…" : "翻訳 (Claude)"}
                </Button>
              }
            >
              <RegenerateConfirm
                label="翻訳"
                trigger="再翻訳 (Claude)"
                busyText="翻訳中…"
                busy={busy() === "translate"}
                disabled={busy() !== null}
                variant="outline"
                onConfirm={() => run("translate", true)}
              />
            </Show>
            {/* Instapaper 転送（旧「後で読む」）。ローカル保存の新機能と名前が
                衝突するため改名。Instapaper インポート完了後に削除予定。 */}
            <Button
              size="sm"
              variant="outline"
              onClick={saveLater}
              disabled={savingLater() || later()?.status === "added"}
            >
              {savingLater()
                ? "送信中…"
                : later()?.status === "added"
                  ? "Instapaper 済 ✓"
                  : later()?.status === "failed"
                    ? "Instapaper 再試行"
                    : "Instapaper へ送る"}
            </Button>
            {/* 後で読む（ローカル保存）: /saved 配下で開いたときだけのアクション */}
            <Show when={savedScope()}>
              <Button
                size="sm"
                variant="outline"
                onClick={toggleArchive}
                disabled={savedBusy()}
              >
                {savedBusy()
                  ? "更新中…"
                  : savedScope()?.archived
                    ? "マイリストへ戻す"
                    : "アーカイブ"}
              </Button>
              <Dialog>
                <DialogTrigger
                  class={buttonVariants({ size: "sm", variant: "ghost" })}
                  disabled={savedBusy()}
                >
                  削除
                </DialogTrigger>
                <DialogContent>
                  <DialogTitle>保存したページを削除しますか？</DialogTitle>
                  <DialogDescription>
                    ページ本体と、付随するスター・タグ・ハイライト・要約もまとめて削除します。
                    この操作は取り消せません。
                  </DialogDescription>
                  <div class="mt-4 flex justify-end gap-2">
                    <DialogCloseTrigger
                      class={buttonVariants({ size: "sm", variant: "outline" })}
                    >
                      キャンセル
                    </DialogCloseTrigger>
                    <DialogCloseTrigger
                      class={buttonVariants({
                        size: "sm",
                        variant: "destructive",
                      })}
                      onClick={() => void deleteSaved()}
                    >
                      削除する
                    </DialogCloseTrigger>
                  </div>
                </DialogContent>
              </Dialog>
            </Show>
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

          {/* 保存ページで本文が未抽出（背景抽出が未完了 or 失敗）のヒント */}
          <Show when={savedScope() && !a().full_content && !a().content}>
            <p class="text-xs text-muted-foreground">
              本文を抽出中です。表示されない場合は「全文を取得」で再試行するか、元記事をお開きください。
            </p>
          </Show>

          <Show when={later()?.status === "failed" && later()?.last_error}>
            <p class="text-xs text-muted-foreground">保存に失敗: {later()?.last_error}</p>
          </Show>

          {/* リッスンモード（#33 v1）: 本文/要約/翻訳をソース切替で読み上げ。
              読み上げコントロールは要約・翻訳より上に置く（操作導線を先頭に集約）。
              本文のみ進捗 80% で既読化（markReadNow）に繋ぐ。バックエンド非依存。 */}
          <ListenBar
            articleId={a().id}
            sources={listenSources}
            onListened={() => markReadNow(a().id)}
          />

          <Show when={a().summary}>
            <Collapsible defaultOpen>
              <section class="rounded-lg border border-border bg-muted/40 p-4">
                <div class="mb-1 flex items-center justify-between gap-2">
                  <CollapsibleTrigger class="-ml-2 flex-1">
                    <CollapsibleIndicator>▾</CollapsibleIndicator>
                    <span class="text-sm font-semibold">要約</span>
                  </CollapsibleTrigger>
                  <DeleteConfirm kind="summary" label="要約" />
                </div>
                {/* 要約は Markdown。renderMarkdown で HTML 化し Prose で描画（コードはハイライト）。 */}
                <CollapsibleContent>
                  <Prose html={summaryHtml()} />
                </CollapsibleContent>
              </section>
            </Collapsible>
          </Show>

          <Show when={a().translation}>
            <Collapsible defaultOpen>
              <section class="rounded-lg border border-border bg-muted/40 p-4">
                <div class="mb-1 flex items-center justify-between gap-2">
                  <CollapsibleTrigger class="-ml-2 flex-1">
                    <CollapsibleIndicator>▾</CollapsibleIndicator>
                    <span class="text-sm font-semibold">翻訳</span>
                  </CollapsibleTrigger>
                  <DeleteConfirm kind="translation" label="翻訳" />
                </div>
                {/* 翻訳も Markdown。renderMarkdown→Prose で HTML 化＋コードハイライト。 */}
                <CollapsibleContent>
                  <Prose html={translationHtml()} />
                </CollapsibleContent>
              </section>
            </Collapsible>
          </Show>

          {/* 本文は信頼できない HTML。bodyHtml で必ず浄化してから Prose に渡す
              （埋め込み <style> によるレイアウト破壊・XSS 対策）。Prose がコードブロックを
              highlight.js で色付けする。full_content があれば優先表示（抜粋トグル時は content）。 */}
          <Prose html={bodyHtml(a())} />

          {/* タグ編集 + AI 提案（#24） */}
          <TagEditor articleId={a().id} />

          {/* ハイライト / 注釈（#32）: 本文選択 → quote 保存 + メモ */}
          <Highlights articleId={a().id} />

          {/* Ask Claude（#22）: 記事本文を context にした対話 Q&A */}
          <ArticleAsk articleId={a().id} />
        </article>
      )}
    </Show>
  );
}
