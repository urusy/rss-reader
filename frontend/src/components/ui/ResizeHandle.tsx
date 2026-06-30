import { cn } from "@/lib/utils";
import type { ResizableWidth } from "@/lib/resizable";

interface ResizeHandleProps {
  /** createResizableWidth の戻り値。幅・ドラッグ・キーボードを供給する。 */
  control: ResizableWidth;
  /** スクリーンリーダ向けの説明（例: "サイドバーの幅"）。 */
  label: string;
  /** このブレークポイント以上で表示（隣の grid が有効になる起点に合わせる）。既定 md。 */
  showFrom?: "md" | "lg";
  class?: string;
}

/**
 * 隣り合う2ペインの境界に絶対配置する縦の区切りハンドル。
 * - 既存の border-r 上に重なる透明なグラブ帯。ホバー/フォーカスで accent 線を表示。
 * - ドラッグ／矢印キー（Home/End）で左ペイン幅を変える（`role="separator"`）。
 * - md 未満は非表示（モバイルは drawer + master-detail のため幅調節は不要）。
 *
 * 親 grid 側で幅の CSS 変数（例: --sidebar-w）を style で与え、その変数を
 * grid-template-columns の左カラムに使い、かつ position: relative にしておくこと
 * （seam = 左ペイン幅の位置に left で合わせるため）。
 */
export function ResizeHandle(props: ResizeHandleProps) {
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      aria-valuenow={Math.round(props.control.width())}
      aria-valuemin={props.control.min}
      aria-valuemax={props.control.max}
      tabindex={0}
      onPointerDown={(e) => props.control.startDrag(e)}
      onKeyDown={(e) => props.control.onKeyDown(e)}
      style={{ left: `${props.control.width()}px` }}
      class={cn(
        "absolute inset-y-0 z-20 hidden w-2 -translate-x-1/2 cursor-col-resize select-none touch-none",
        (props.showFrom ?? "md") === "lg" ? "lg:block" : "md:block",
        "focus:outline-none",
        // 既存 border に重なる細い線。普段は透明、ホバー/フォーカスで accent。
        "after:absolute after:inset-y-0 after:left-1/2 after:w-0.5 after:-translate-x-1/2 after:bg-transparent after:transition-colors after:content-['']",
        "hover:after:bg-ring/60 focus-visible:after:bg-ring",
        props.class,
      )}
    />
  );
}
