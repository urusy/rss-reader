import { useApp } from "@/lib/store";
import { cn } from "@/lib/utils";

const opt = "flex-1 h-7 rounded text-xs font-medium transition-colors";

/** すべて / 未読のみ の表示切り替え（機能11）。状態は store.filter。 */
export function FilterToggle() {
  const app = useApp();
  return (
    <div class="flex gap-0.5 rounded-md bg-muted p-0.5">
      <button
        type="button"
        class={cn(
          opt,
          app.state.filter === "all"
            ? "bg-background shadow-sm"
            : "text-muted-foreground",
        )}
        onClick={() => app.setFilter("all")}
      >
        すべて
      </button>
      <button
        type="button"
        class={cn(
          opt,
          app.state.filter === "unread"
            ? "bg-background shadow-sm"
            : "text-muted-foreground",
        )}
        onClick={() => app.setFilter("unread")}
      >
        未読のみ
      </button>
    </div>
  );
}
