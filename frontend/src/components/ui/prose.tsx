import { createEffect } from "solid-js";
import { cn } from "@/lib/utils";
import { highlightWithin } from "@/lib/highlight";

/**
 * 浄化済み HTML を `prose`（Tailwind Typography）で描画し、描画後に
 * highlight.js でコードブロックを色付けする共通コンポーネント。
 * 記事本文・要約・翻訳・ダイジェストの表示を 1 経路に統一する。
 *
 * innerHTML の反映と直後のハイライトを 1 つの effect で確定させ、順序依存を排除する
 * （Solid の innerHTML バインディングに任せず、el.innerHTML を自分で入れてから走らせる）。
 */
export function Prose(props: { html: string; class?: string }) {
  let el: HTMLDivElement | undefined;
  createEffect(() => {
    const html = props.html;
    if (!el) return;
    el.innerHTML = html;
    highlightWithin(el);
  });
  return (
    <div
      ref={el}
      class={cn("prose prose-sm dark:prose-invert max-w-none", props.class)}
    />
  );
}
