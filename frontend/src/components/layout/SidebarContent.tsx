import { createSignal } from "solid-js";
import { A } from "@solidjs/router";
import { useApp } from "@/lib/store";
import { api } from "@/lib/api";
import FeedTree from "./FeedTree";
import { ThemeToggle } from "./ThemeToggle";
import { FilterToggle } from "./FilterToggle";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const navItem = "block h-8 px-2 rounded-md text-sm leading-8 hover:bg-accent";
const navActive = "bg-accent text-accent-foreground";

/** Sidebar の中身（デスクトップ aside とモバイルドロワーで共有）。 */
export default function SidebarContent(props: { onNavigate?: () => void }) {
  const app = useApp();
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const go = () => props.onNavigate?.();

  const addFeed = async () => {
    const v = url().trim();
    if (!v) return;
    setBusy(true);
    try {
      await api.addFeed(v);
      setUrl("");
      app.refetchFeeds();
    } catch (e) {
      alert(`追加に失敗しました: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="flex h-full flex-col gap-3 p-3">
      <A
        href="/"
        class="px-2 text-lg font-semibold tracking-tight"
        onClick={go}
      >
        RSS Reader
      </A>

      <FilterToggle />

      <A href="/" end class={navItem} activeClass={navActive} onClick={go}>
        すべての記事
      </A>

      <div class="flex-1 overflow-y-auto">
        <FeedTree onNavigate={props.onNavigate} />
      </div>

      <div class="space-y-2 border-t border-border pt-3">
        <div class="flex gap-1">
          <Input
            placeholder="https://example.com/feed.xml"
            value={url()}
            onInput={(e) => setUrl(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && addFeed()}
          />
          <Button size="sm" onClick={addFeed} disabled={busy()}>
            {busy() ? "…" : "追加"}
          </Button>
        </div>
        <div class="flex flex-col gap-0.5">
          <A href="/manage" class={navItem} activeClass={navActive} onClick={go}>
            フィード管理
          </A>
          <A href="/settings" class={navItem} activeClass={navActive} onClick={go}>
            設定
          </A>
        </div>
        <div class="px-2">
          <ThemeToggle />
        </div>
      </div>
    </div>
  );
}
