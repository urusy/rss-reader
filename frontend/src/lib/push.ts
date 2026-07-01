// #31 Web Push クライアント。SW 登録・購読フロー・許可状態の薄いラッパ。
// 純粋な base64url 変換だけ切り出して vitest 対象にする（push.test.ts）。

import { api } from "./api";

/** Web Push が使える環境か（SW + PushManager + Notification が揃うか）。 */
export function pushSupported(): boolean {
  return (
    typeof navigator !== "undefined" &&
    "serviceWorker" in navigator &&
    typeof window !== "undefined" &&
    "PushManager" in window &&
    "Notification" in window
  );
}

/**
 * base64url な VAPID 公開鍵を applicationServerKey 用の Uint8Array に変換する（純関数）。
 * `-`/`_` を base64 標準へ戻し、パディングを補ってからデコードする。
 */
export function urlBase64ToUint8Array(base64String: string): Uint8Array {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(base64);
  // ArrayBuffer を明示確保（applicationServerKey は ArrayBuffer 裏付けの BufferSource を要求）。
  const output = new Uint8Array(new ArrayBuffer(raw.length));
  for (let i = 0; i < raw.length; i++) output[i] = raw.charCodeAt(i);
  return output;
}

export type PushPermission = "granted" | "denied" | "default" | "unsupported";

/** 現在の通知許可状態。 */
export function currentPermission(): PushPermission {
  if (!pushSupported()) return "unsupported";
  return Notification.permission as PushPermission;
}

/** SW を登録（対応環境のみ）。失敗は null。 */
export async function registerServiceWorker(): Promise<ServiceWorkerRegistration | null> {
  if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return null;
  try {
    return await navigator.serviceWorker.register("/sw.js");
  } catch (e) {
    console.error("[push] service worker registration failed", e);
    return null;
  }
}

/** 既に購読済みか。 */
export async function isSubscribed(): Promise<boolean> {
  if (!pushSupported()) return false;
  const reg = await navigator.serviceWorker.getRegistration();
  const sub = await reg?.pushManager.getSubscription();
  return !!sub;
}

/**
 * 通知を許可し、購読を作成してサーバへ登録する。
 * 許可されなかった場合は false。VAPID 未設定なら getPushPublicKey が 503 を投げる。
 */
export async function enablePush(): Promise<boolean> {
  if (!pushSupported()) throw new Error("このブラウザは Web Push に対応していません");
  const { public_key } = await api.getPushPublicKey();
  const reg = await registerServiceWorker();
  if (!reg) throw new Error("Service Worker を登録できませんでした");
  await navigator.serviceWorker.ready;

  const permission = await Notification.requestPermission();
  if (permission !== "granted") return false;

  const existing = await reg.pushManager.getSubscription();
  const sub =
    existing ??
    (await reg.pushManager.subscribe({
      userVisibleOnly: true,
      // lib.dom の BufferSource は ArrayBuffer 裏付けを要求。ArrayBuffer 確保済みなので cast。
      applicationServerKey: urlBase64ToUint8Array(public_key) as BufferSource,
    }));
  await api.subscribePush(sub.toJSON());
  return true;
}

/** 購読を解除（ブラウザ側 + サーバ側）。 */
export async function disablePush(): Promise<void> {
  if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return;
  const reg = await navigator.serviceWorker.getRegistration();
  const sub = await reg?.pushManager.getSubscription();
  if (!sub) return;
  const endpoint = sub.endpoint;
  await sub.unsubscribe().catch(() => {});
  await api.unsubscribePush(endpoint).catch(() => {});
}
