import type { ParentComponent } from "solid-js";
import { AppProvider } from "@/lib/store";
import Sidebar from "@/components/layout/Sidebar";
import MobileTopBar from "@/components/layout/MobileTopBar";

/** 二ペインのアプリシェル。左 Sidebar（永続）+ 右ペイン（一覧 or 本文）。 */
const App: ParentComponent = (props) => (
  <AppProvider>
    <div class="min-h-screen bg-background text-foreground md:grid md:grid-cols-[280px_1fr]">
      <Sidebar />
      <div class="flex min-h-screen min-w-0 flex-col">
        <MobileTopBar />
        <main class="mx-auto w-full max-w-3xl flex-1 px-4 py-6">
          {props.children}
        </main>
      </div>
    </div>
  </AppProvider>
);

export default App;
