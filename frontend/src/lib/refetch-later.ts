/**
 * フィード追加直後の初回クロールはバックエンドで背景実行される（POST は即返る）。
 * タイトルや未読数はクロール完了後に確定するため、追加操作の後しばらくしてから
 * 再取得して画面へ反映する。既定の 3s / 10s は「たいていのフィードは数秒で
 * クロールが終わる + 遅いサイトの取りこぼし保険」の二段構え。
 */
export function scheduleFollowUpRefetch(
  refetch: () => void,
  delays: readonly number[] = [3000, 10000],
): () => void {
  const timers = delays.map((d) => setTimeout(refetch, d));
  return () => timers.forEach((t) => clearTimeout(t));
}
