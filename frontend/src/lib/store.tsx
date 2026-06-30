import {
  createContext,
  useContext,
  createResource,
  type ParentComponent,
  type Resource,
} from "solid-js";
import { createStore } from "solid-js/store";
import { api, type Feed, type Folder } from "@/lib/api";

export interface UiState {
  sidebarOpen: boolean; // モバイルドロワー
  filter: "all" | "unread"; // #11 が使用（すべて/未読トグル）
  // このセッションで既読化した記事ID。本文ペイン（滞在/スクロール起点）で立て、
  // 一覧ペインが行のグレーアウト判定に使う（兄弟ペインは別 resource なので共有が要る）。
  readIds: Record<string, true>;
}

export interface UiStore {
  state: UiState;
  openSidebar(): void;
  closeSidebar(): void;
  toggleSidebar(): void;
  setFilter(f: "all" | "unread"): void;
  markReadLocal(id: string): void; // 本文ペインが実既読の瞬間に呼ぶ
  feeds: Resource<Feed[]>; // Sidebar が2箇所で共有する単一リソース
  folders: Resource<Folder[]>;
  refetchFeeds(): void; // フィード追加後などに呼ぶ
  refetchFolders(): void;
}

const Ctx = createContext<UiStore>();

export const AppProvider: ParentComponent = (props) => {
  const [state, setState] = createStore<UiState>({
    sidebarOpen: false,
    filter: "all",
    readIds: {},
  });
  const [feeds, { refetch: refetchFeeds }] = createResource(
    () => api.listFeeds(),
    { initialValue: [] },
  );
  const [folders, { refetch: refetchFolders }] = createResource(
    () => api.listFolders(),
    { initialValue: [] },
  );

  const store: UiStore = {
    state,
    openSidebar: () => setState("sidebarOpen", true),
    closeSidebar: () => setState("sidebarOpen", false),
    toggleSidebar: () => setState("sidebarOpen", (v) => !v),
    setFilter: (f) => setState("filter", f),
    markReadLocal: (id) => setState("readIds", id, true),
    feeds,
    folders,
    refetchFeeds: () => {
      void refetchFeeds();
    },
    refetchFolders: () => {
      void refetchFolders();
    },
  };
  return <Ctx.Provider value={store}>{props.children}</Ctx.Provider>;
};

export function useApp(): UiStore {
  const v = useContext(Ctx);
  if (!v) throw new Error("useApp must be used within <AppProvider>");
  return v;
}
