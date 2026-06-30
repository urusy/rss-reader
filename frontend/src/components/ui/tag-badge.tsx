import { splitProps, type ComponentProps, Show } from "solid-js";
import { cn } from "@/lib/utils";

type Props = ComponentProps<"span"> & {
  color?: string | null;
  ai?: boolean; // AI 由来は破線枠で薄く区別
  onRemove?: () => void;
};

export function TagBadge(props: Props) {
  const [local, rest] = splitProps(props, [
    "class",
    "color",
    "ai",
    "onRemove",
    "children",
  ]);
  return (
    <span
      class={cn(
        "inline-flex items-center gap-1 rounded-full border border-border bg-muted px-2 py-0.5 text-xs text-foreground",
        local.ai && "border-dashed text-muted-foreground",
        local.class,
      )}
      style={local.color ? { "border-color": local.color } : undefined}
      {...rest}
    >
      {local.children}
      <Show when={local.onRemove}>
        <button
          type="button"
          class="opacity-60 hover:opacity-100"
          onClick={() => local.onRemove?.()}
        >
          ×
        </button>
      </Show>
    </span>
  );
}
