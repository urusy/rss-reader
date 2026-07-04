// Collapsible — wraps Ark UI's headless collapsible and styles it with our tokens.
//
// Why Ark UI (not shadcn-solid/Kobalte): see components/ui/dialog.tsx の冒頭。
// ヘッダをクリックすると本文を開閉できる。トリガ内に別の操作要素（削除ボタン等）を
// 入れ子にしないこと（ネストしたインタラクティブ要素は a11y 上不正）。ヘッダ行では
// CollapsibleTrigger と操作ボタンを横並びの兄弟として置く。
//
// NOTE: Ark UI の compound API はメジャー更新で変わりうる。壊れたら ark-ui.com
// （Solid / Collapsible）で現行の形を確認する。
//
// Usage:
//   <Collapsible defaultOpen>
//     <div class="flex items-center justify-between">
//       <CollapsibleTrigger>
//         <CollapsibleIndicator>▾</CollapsibleIndicator>
//         <span>要約</span>
//       </CollapsibleTrigger>
//       <Button variant="ghost">削除</Button>
//     </div>
//     <CollapsibleContent>…本文…</CollapsibleContent>
//   </Collapsible>

import { Collapsible as ArkCollapsible } from "@ark-ui/solid/collapsible";
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export const Collapsible = ArkCollapsible.Root;

/** ヘッダのクリック領域。開閉トグル。中に別の button を入れ子にしない。 */
export function CollapsibleTrigger(
  props: ComponentProps<typeof ArkCollapsible.Trigger>,
) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <ArkCollapsible.Trigger
      class={cn(
        // クリック領域を広く分かりやすく: padding + hover 背景 + pointer カーソル。
        "flex items-center gap-1.5 rounded px-2 py-1 text-left cursor-pointer select-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        local.class,
      )}
      {...rest}
    />
  );
}

/** 開閉状態で回転する▾インジケータ（open=下向き / closed=右向き）。 */
export function CollapsibleIndicator(
  props: ComponentProps<typeof ArkCollapsible.Indicator>,
) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <ArkCollapsible.Indicator
      class={cn(
        "inline-block text-xs text-muted-foreground transition-transform data-[state=closed]:-rotate-90",
        local.class,
      )}
      {...rest}
    />
  );
}

/** 開閉する本文。閉じると DOM から外れる（Ark の既定 render strategy）。 */
export function CollapsibleContent(
  props: ComponentProps<typeof ArkCollapsible.Content>,
) {
  return <ArkCollapsible.Content {...props} />;
}
