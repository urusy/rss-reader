import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export function Input(props: ComponentProps<"input">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <input
      class={cn(
        "flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm",
        // pointer-coarse:min-h-11 — タッチ端末のみ最小高さ 44px（デスクトップは h-9 のまま）。
        "pointer-coarse:min-h-11",
        "placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2",
        "focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        local.class,
      )}
      {...rest}
    />
  );
}
