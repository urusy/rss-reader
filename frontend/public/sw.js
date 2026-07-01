/* #31 Web Push service worker.
 * push       -> 通知を表示（payload は notifications スライスの JSON: {title, body, url}）
 * notificationclick -> 該当記事 URL を既存タブでフォーカス or 新規に開く
 * fetch ハンドラは持たない（オフラインキャッシュ／リクエスト横取りはしない）。
 */

self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch (_e) {
    data = { title: "新着記事", body: event.data ? event.data.text() : "", url: "/" };
  }
  const title = data.title || "新着記事";
  const options = {
    body: data.body || "",
    data: { url: data.url || "/" },
    icon: "/icon-192.png",
    badge: "/icon-192.png",
    // 同じ記事の重複通知を1枚にまとめる。
    tag: data.url || undefined,
  };
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = (event.notification.data && event.notification.data.url) || "/";
  event.waitUntil(
    self.clients
      .matchAll({ type: "window", includeUncontrolled: true })
      .then((clientList) => {
        for (const client of clientList) {
          if ("focus" in client) {
            if ("navigate" in client) client.navigate(url);
            return client.focus();
          }
        }
        if (self.clients.openWindow) return self.clients.openWindow(url);
        return undefined;
      }),
  );
});
