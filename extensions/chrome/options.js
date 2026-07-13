// サーバー URL + SAVE_TOKEN を chrome.storage.local に保存する。
// storage.sync は Google アカウント経由で同期される（= トークンが外へ出る）ため使わない。
// 保存時に対象オリジンのホスト権限を動的取得する（optional_host_permissions）。

const $ = (id) => document.getElementById(id);

async function load() {
  const { server, token } = await chrome.storage.local.get(["server", "token"]);
  if (server) $("server").value = server;
  if (token) $("token").value = token;
}

async function save() {
  const status = $("status");
  const server = $("server").value.trim().replace(/\/$/, "");
  const token = $("token").value.trim();
  if (!server || !token) {
    status.textContent = "両方入力してください";
    return;
  }
  let origin;
  try {
    origin = new URL(server).origin + "/*";
  } catch {
    status.textContent = "URL の形式が正しくありません";
    return;
  }
  const granted = await chrome.permissions.request({ origins: [origin] });
  if (!granted) {
    status.textContent = "サーバーへのアクセス権限が必要です";
    return;
  }
  await chrome.storage.local.set({ server, token });
  status.textContent = "保存しました ✓";
  setTimeout(() => (status.textContent = ""), 2000);
}

document.addEventListener("DOMContentLoaded", () => {
  void load();
  $("save").addEventListener("click", () => void save());
});
