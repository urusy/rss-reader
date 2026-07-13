import { createMemo } from "solid-js";
import { useLocation, useParams } from "@solidjs/router";

export type Scope =
  | { kind: "all" }
  | { kind: "feed"; feedId: string }
  | { kind: "folder"; folderId: string } // folderId は "unclassified" センチネルを取りうる
  | { kind: "view"; viewId: string } // #27 スマートビュー（仮想フィード）
  | { kind: "saved"; archived: boolean }; // 後で読む（保存ページ）。/saved=マイリスト /saved/archive=アーカイブ

/** 純粋関数（vitest 対象）。URL pathname と params から scope を決める。 */
export function scopeFromPath(
  pathname: string,
  params: Record<string, string | undefined>,
): Scope {
  if (pathname.startsWith("/feeds/") && params.feedId)
    return { kind: "feed", feedId: params.feedId };
  if (pathname.startsWith("/folders/") && params.folderId)
    return { kind: "folder", folderId: params.folderId };
  if (pathname.startsWith("/views/") && params.viewId)
    return { kind: "view", viewId: params.viewId };
  if (pathname === "/saved/archive") return { kind: "saved", archived: true };
  if (pathname.startsWith("/saved")) return { kind: "saved", archived: false };
  return { kind: "all" };
}

export function useSelection(): () => Scope {
  const loc = useLocation();
  const params = useParams();
  return createMemo(() => scopeFromPath(loc.pathname, params));
}
