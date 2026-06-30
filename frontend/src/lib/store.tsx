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
  // 中央一覧の現在の表示順（ArticleList が書き、キーボードハンドラが j/k/o/Enter で読む）。
  // o（原文）のため url も持つ。readIds と同型の兄弟ペイン間共有。#18
  navItems: { id: string; url: string }[];
  // ? のチートシート overlay 開閉。#18
  helpOpen: boolean;
}

export interface UiStore {
  state: UiState;
  openSidebar(): void;
  closeSidebar(): void;
  toggleSidebar(): void;
  setFilter(f: "all" | "unread"): void;
  markReadLocal(id: string): void; // 本文ペインが実既読の瞬間に呼ぶ
  setNavItems(items: { id: string; url: string }[]): void; // #18
  toggleHelp(): void; // #18
  closeHelp(): void; // #18
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
    navItems: [],
    helpOpen: false,
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
    setNavItems: (items) => setState("navItems", items),
    toggleHelp: () => setState("helpOpen", (v) => !v),
    closeHelp: () => setState("helpOpen", false),
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
