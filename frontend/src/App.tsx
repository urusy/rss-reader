import type { ParentComponent } from "solid-js";
import { AppProvider } from "@/lib/store";
import Sidebar from "@/components/layout/Sidebar";
import MobileTopBar from "@/components/layout/MobileTopBar";
import { ResizeHandle } from "@/components/ui/ResizeHandle";
import { createResizableWidth } from "@/lib/resizable";
import LoginGate from "@/components/auth/LoginGate";

/** 二ペインのアプリシェル。左 Sidebar（永続・幅調節可）+ 右ペイン（一覧 or 本文）。 */
const App: ParentComponent = (props) => {
  // サイドバーの幅は全ルート共通。ドラッグ/矢印キーで調節し localStorage に永続化。
  const sidebar = createResizableWidth({
    storageKey: "sidebar-w",
    defaultWidth: 280,
    min: 200,
    max: 480,
  });

  return (
    <AppProvider>
      <LoginGate>
        <div
          class="relative min-h-dvh bg-background text-foreground lg:grid lg:grid-cols-[var(--sidebar-w)_1fr]"
          style={{ "--sidebar-w": `${sidebar.width()}px` }}
        >
          <Sidebar />
          <ResizeHandle control={sidebar} label="サイドバーの幅" showFrom="lg" />
          {/* md+ は列を画面高に固定（md:h-dvh）。上部バー(lg未満で表示)を引いた残りを
              main が flex-1 で埋め、main 自身がスクロール領域になる。<md はボディスクロール。 */}
          <div class="flex min-h-dvh min-w-0 flex-col md:h-dvh">
            <MobileTopBar />
            {/* 全幅。狭い読み物ページ（設定/管理/検索/本文）は各ルートが自前で max-w-3xl
                ラッパを持つ。Reader は h-full で main を埋め、ペインが内部スクロールする。
                pb-safe: 最下部をホームインジケータから逃がす（<md=ボディ末尾 / md+=main末尾）。 */}
            <main class="min-w-0 flex-1 pb-safe md:min-h-0 md:overflow-y-auto">
              {props.children}
            </main>
          </div>
        </div>
      </LoginGate>
    </AppProvider>
  );
};

export default App;
