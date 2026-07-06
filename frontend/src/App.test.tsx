// App シェルの認証ゲート順序の検証。
// バグ: AppProvider が LoginGate の外側にあると、未認証のページ読込時に
// 保護 API (feeds/folders/saved-views/relevance) が 401 で先走り、
// ログイン成功後も再取得されずサイドバーが空のままになる。
// 契約: ①未認証中は保護 API を呼ばない ②ログイン成功後に初めて取得する。
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  render,
  screen,
  waitFor,
  fireEvent,
  cleanup,
} from "@solidjs/testing-library";
import { MemoryRouter, Route } from "@solidjs/router";
import App from "./App";
import { api } from "@/lib/api";
import { setAuthState } from "@/lib/auth";

// --- 重い子コンポーネントはスタブ（本テストの関心はゲート順序のみ） ---
vi.mock("@/components/layout/Sidebar", () => ({
  default: () => <div data-testid="sidebar" />,
}));
vi.mock("@/components/layout/MobileTopBar", () => ({
  default: () => <div />,
}));
vi.mock("@/components/keyboard/KeyboardHelp", () => ({
  default: () => <div />,
}));
vi.mock("@/lib/keyboard", () => ({
  useKeyboardShortcuts: () => {},
}));

// --- api は全面モック（ネットワーク禁止） ---
vi.mock("@/lib/api", () => ({
  api: {
    getAuthStatus: vi.fn(),
    login: vi.fn(),
    setupPassword: vi.fn(),
    listFeeds: vi.fn(async () => []),
    listFolders: vi.fn(async () => []),
    listSavedViews: vi.fn(async () => []),
    listRelevanceScores: vi.fn(async () => []),
  },
  errorStatus: (e: unknown) =>
    typeof e === "object" && e !== null && "status" in e
      ? (e as { status: number }).status
      : undefined,
}));

const mocked = vi.mocked(api);

function renderApp() {
  return render(() => (
    <MemoryRouter root={App}>
      <Route path="/" component={() => <div data-testid="content" />} />
    </MemoryRouter>
  ));
}

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  setAuthState("unknown"); // モジュールスコープの signal を毎回リセット
});

describe("App の認証ゲート", () => {
  it("未認証の間は保護 API を呼ばない", async () => {
    mocked.getAuthStatus.mockResolvedValue({
      setup_required: false,
      authenticated: false,
    });
    renderApp();
    await screen.findByPlaceholderText("パスワード"); // ログインフォーム表示まで待つ
    expect(mocked.listFeeds).not.toHaveBeenCalled();
    expect(mocked.listFolders).not.toHaveBeenCalled();
    expect(mocked.listSavedViews).not.toHaveBeenCalled();
    expect(mocked.listRelevanceScores).not.toHaveBeenCalled();
  });

  it("ログイン成功後にデータ取得が走り、本体が描画される", async () => {
    mocked.getAuthStatus.mockResolvedValue({
      setup_required: false,
      authenticated: false,
    });
    mocked.login.mockResolvedValue({ ok: true });
    renderApp();
    await screen.findByPlaceholderText("パスワード");
    mocked.listFeeds.mockClear(); // ここまでの先走り分を無視し「ログイン後」だけ数える

    fireEvent.input(screen.getByPlaceholderText("パスワード"), {
      target: { value: "correct-password" },
    });
    fireEvent.click(screen.getByRole("button", { name: "サインイン" }));

    await waitFor(() => expect(mocked.listFeeds).toHaveBeenCalled());
    await screen.findByTestId("content"); // 認証済み: ルート本体が出る
    expect(screen.getByTestId("sidebar")).toBeTruthy();
  });

  it("認証済みセッションがあれば即座に本体とデータ取得", async () => {
    mocked.getAuthStatus.mockResolvedValue({
      setup_required: false,
      authenticated: true,
    });
    renderApp();
    await screen.findByTestId("content");
    await waitFor(() => expect(mocked.listFeeds).toHaveBeenCalled());
  });
});
