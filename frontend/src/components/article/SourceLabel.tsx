import { createEffect, createSignal, Show } from "solid-js";
import { faviconUrlFor, sourceInitial, avatarColor } from "@/lib/favicon";

/**
 * 記事一覧の各行に付けるソース表示（issue #1）。フィードのサイト favicon を
 * 記事 URL のオリジンから直読みし、取得できない場合は頭文字＋自動色の丸
 * アバターにフォールバックする。追加のネットワーク依存は購読済みサイトの
 * /favicon.ico のみ（第三者サービス不使用・CSP は img-src で許可済み）。
 */
export default function SourceLabel(props: { name: string; url: string }) {
  const [failed, setFailed] = createSignal(false);
  const icon = () => faviconUrlFor(props.url);
  // 行が別記事に切り替わって URL が変わったら失敗フラグを畳み直す。
  createEffect(() => {
    props.url;
    setFailed(false);
  });
  const showImg = () => !!icon() && !failed();

  return (
    <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
      <Show
        when={showImg()}
        fallback={
          <span
            class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[9px] font-semibold leading-none text-white"
            style={{ "background-color": avatarColor(props.name) }}
            aria-hidden="true"
          >
            {sourceInitial(props.name)}
          </span>
        }
      >
        <img
          src={icon()!}
          alt=""
          width="16"
          height="16"
          loading="lazy"
          class="h-4 w-4 shrink-0 rounded-sm object-contain"
          onError={() => setFailed(true)}
        />
      </Show>
      <span class="truncate">{props.name}</span>
    </div>
  );
}
