import { cva } from "class-variance-authority";
import type { JSX } from "solid-js";
import { cn } from "@/lib/utils";

const badge = cva(
  "inline-flex shrink-0 items-center whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium tabular-nums",
  {
    variants: {
      variant: {
        default: "bg-muted text-muted-foreground",
        unread: "bg-accent text-accent-foreground", // 未読あり強調
        stale: "bg-muted text-muted-foreground ring-1 ring-border", // 更新停滞：控えめ
        dead: "bg-destructive text-destructive-foreground", // 取得失敗：強調
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export function Badge(props: {
  class?: string;
  variant?: "default" | "unread" | "stale" | "dead";
  title?: string;
  children: JSX.Element;
}) {
  return (
    <span
      class={cn(badge({ variant: props.variant }), props.class)}
      title={props.title}
    >
      {props.children}
    </span>
  );
}
