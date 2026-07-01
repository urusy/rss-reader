import { A } from "@solidjs/router";
import FeedTree from "./FeedTree";
import SavedViewList from "./SavedViewList";
import { ThemeToggle } from "./ThemeToggle";
import { FilterToggle } from "./FilterToggle";
import { AddFeedDialog } from "./AddFeedDialog";
import { SearchBox } from "./SearchBox";

const navItem = "block h-8 px-2 rounded-md text-sm leading-8 hover:bg-accent";
const navActive = "bg-accent text-accent-foreground";

/** Sidebar の中身（デスクトップ aside とモバイルドロワーで共有）。 */
export default function SidebarContent(props: { onNavigate?: () => void }) {
  const go = () => props.onNavigate?.();

  return (
    <div class="flex h-full flex-col gap-3 p-3">
      <A
        href="/"
        class="px-2 text-lg font-semibold tracking-tight"
        onClick={go}
      >
        RSS Reader
      </A>

      <SearchBox onNavigate={props.onNavigate} />

      <FilterToggle />

      <A href="/" end class={navItem} activeClass={navActive} onClick={go}>
        すべての記事
      </A>

      <div class="flex-1 space-y-3 overflow-y-auto">
        <SavedViewList onNavigate={props.onNavigate} />
        <FeedTree onNavigate={props.onNavigate} />
      </div>

      <div class="space-y-2 border-t border-border pt-3">
        <AddFeedDialog />
        <div class="flex flex-col gap-0.5">
          <A href="/manage" class={navItem} activeClass={navActive} onClick={go}>
            フィード管理
          </A>
          <A href="/rules" class={navItem} activeClass={navActive} onClick={go}>
            自動ルール
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
