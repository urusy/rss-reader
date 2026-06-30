import { useParams } from "@solidjs/router";
import ArticleDetail from "@/components/article/ArticleDetail";

/** 単体ページ /articles/:id（検索結果・直リンク用）。本文は ArticleDetail を共用。 */
export default function ArticleView() {
  const params = useParams();
  return (
    <div class="mx-auto max-w-3xl px-4 py-6">
      <ArticleDetail id={params.id} />
    </div>
  );
}
