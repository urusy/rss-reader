// アクションクリック → 現在タブの URL を POST /api/save（Bearer SAVE_TOKEN）。
// MV3 の service worker からの fetch はホスト権限があれば CORS 検査を受けない
// ため、サーバ側の CORS 設定は不要（options.js が保存時に権限を動的取得する）。

const BADGE_MS = 2000;

function badge(tabId, text, color) {
  chrome.action.setBadgeBackgroundColor({ tabId, color });
  chrome.action.setBadgeText({ tabId, text });
  setTimeout(() => chrome.action.setBadgeText({ tabId, text: "" }), BADGE_MS);
}

chrome.action.onClicked.addListener(async (tab) => {
  const tabId = tab.id;
  const url = tab.url ?? "";
  if (!/^https?:\/\//.test(url)) {
    badge(tabId, "×", "#991b1b"); // chrome:// 等は保存対象外
    return;
  }

  const { server, token } = await chrome.storage.local.get(["server", "token"]);
  if (!server || !token) {
    badge(tabId, "!", "#b45309");
    chrome.runtime.openOptionsPage(); // 未設定なら設定画面へ誘導
    return;
  }

  try {
    const res = await fetch(`${server.replace(/\/$/, "")}/api/save`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ url }),
    });
    if (res.ok) {
      badge(tabId, "✓", "#166534");
    } else {
      badge(tabId, String(res.status), "#991b1b");
    }
  } catch (e) {
    console.error("save failed", e);
    badge(tabId, "!", "#991b1b");
  }
});
