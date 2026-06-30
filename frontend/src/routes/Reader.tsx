import { Show } from "solid-js";
import { useSearchParams } from "@solidjs/router";
import ArticleList from "./ArticleList";
import ArticleDetail from "@/components/article/ArticleDetail";
import { Button } from "@/components/ui/button";
import { ResizeHandle } from "@/components/ui/ResizeHandle";
import { createResizableWidth } from "@/lib/resizable";
import { cn } from "@/lib/utils";

/**
 * 3ペインの右側2枚（中央=記事一覧 / 右=本文）。左のサイドバーは App シェルが持つ。
 * - スコープ（feed/folder）はパス、選択中の記事は ?article=<id> クエリで表現する。
 * - md+ は grid の3ペイン（各ペイン独立スクロール）。
 * - モバイルは記事一覧 ⇆ 本文の master-detail 切替（「← 記事一覧へ」で戻る）。
 */
export default function Reader() {
  const [searchParams, setSearchParams] = useSearchParams();
  const selectedId = () =>
    Array.isArray(searchParams.article)
      ? searchParams.article[0]
      : searchParams.article;
  const clearSelection = () => setSearchParams({ article: null });

  // 記事一覧ペインの幅。ドラッグ/矢印キーで調節し localStorage に永続化。
  const list = createResizableWidth({
    storageKey: "list-w",
    defaultWidth: 340,
    min: 280,
    max: 560,
  });

  return (
    <div
      class="relative flex flex-col md:grid md:h-full md:grid-cols-[var(--list-w)_1fr] md:overflow-hidden"
      style={{ "--list-w": `${list.width()}px` }}
    >
      {/* 中央: 記事一覧。記事選択中はモバイルで隠す（master-detail）。 */}
      <div
        class={cn(
          "min-h-0 border-border md:overflow-y-auto md:border-r",
          selectedId() ? "hidden md:block" : "block",
        )}
      >
        <ArticleList />
      </div>

      <ResizeHandle control={list} label="記事一覧の幅" />

      {/* 右: 選択記事の本文。未選択時はプレースホルダ。 */}
      <div
        class={cn(
          "min-h-0 md:overflow-y-auto",
          selectedId() ? "block" : "hidden md:block",
        )}
      >
        <Show
          when={selectedId()}
          fallback={
            <div class="hidden h-full items-center justify-center p-8 text-sm text-muted-foreground md:flex">
              記事を選択してください。
            </div>
          }
        >
          {(id) => (
            <div class="mx-auto max-w-3xl px-4 py-6">
              <Button
                variant="ghost"
                size="sm"
                class="mb-4 md:hidden"
                onClick={clearSelection}
              >
                ← 記事一覧へ
              </Button>
              <ArticleDetail id={id()} />
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
