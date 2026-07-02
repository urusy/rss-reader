// highlight.js の実行時シンタックスハイライト。core + 主要言語のみ登録して軽量に保つ
// （full ビルドは全言語を抱えて重い）。色はトークン用 class（.hljs-*）に app.css の
// CSS 変数を割り当てて、アプリのテーマ（light/dark/graphite/sepia）に追従させる。
import hljs from "highlight.js/lib/core";

import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import go from "highlight.js/lib/languages/go";
import bash from "highlight.js/lib/languages/bash";
import shell from "highlight.js/lib/languages/shell";
import json from "highlight.js/lib/languages/json";
import yaml from "highlight.js/lib/languages/yaml";
import sql from "highlight.js/lib/languages/sql";
import cpp from "highlight.js/lib/languages/cpp";
import c from "highlight.js/lib/languages/c";
import java from "highlight.js/lib/languages/java";
import csharp from "highlight.js/lib/languages/csharp";
import ruby from "highlight.js/lib/languages/ruby";
import php from "highlight.js/lib/languages/php";
import kotlin from "highlight.js/lib/languages/kotlin";
import swift from "highlight.js/lib/languages/swift";
import xml from "highlight.js/lib/languages/xml"; // html も兼ねる
import css from "highlight.js/lib/languages/css";
import markdown from "highlight.js/lib/languages/markdown";
import diff from "highlight.js/lib/languages/diff";

const LANGS: Record<string, Parameters<typeof hljs.registerLanguage>[1]> = {
  javascript,
  typescript,
  python,
  rust,
  go,
  bash,
  shell,
  json,
  yaml,
  sql,
  cpp,
  c,
  java,
  csharp,
  ruby,
  php,
  kotlin,
  swift,
  xml,
  css,
  markdown,
  diff,
};

let registered = false;
function ensureRegistered() {
  if (registered) return;
  for (const [name, lang] of Object.entries(LANGS)) {
    hljs.registerLanguage(name, lang);
  }
  registered = true;
}

/**
 * root 配下の `<pre><code>` を全てハイライトする。marked が付けた `language-xxx`
 * class があればその言語で、無ければ自動判定で色付けする。innerHTML を差し替える
 * たびに呼ぶ想定なので、二重適用ガード（data-highlighted）を尊重する。
 */
export function highlightWithin(root: HTMLElement): void {
  ensureRegistered();
  root.querySelectorAll<HTMLElement>("pre code").forEach((el) => {
    if (el.dataset.highlighted) return;
    hljs.highlightElement(el);
  });
}
