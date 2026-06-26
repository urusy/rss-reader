import { cva } from "class-variance-authority";
import type { JSX } from "solid-js";
import { cn } from "@/lib/utils";

const badge = cva(
  "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium tabular-nums",
  {
    variants: {
      variant: {
        default: "bg-muted text-muted-foreground",
        unread: "bg-accent text-accent-foreground", // 未読あり強調
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export function Badge(props: {
  class?: string;
  variant?: "default" | "unread";
  children: JSX.Element;
}) {
  return (
    <span class={cn(badge({ variant: props.variant }), props.class)}>
      {props.children}
    </span>
  );
}
