// ゼロ FOUC: バンドル読込前に dark クラス + color-scheme を当てる。
// ランタイムの真実は lib/theme.ts。ここは最初の1フレームの当てのみ。
// index.html のインラインではなく外部ファイルなのは CSP (script-src 'self') のため。
(function () {
  try {
    var valid = ["light", "dark", "graphite", "sepia"];
    var t = localStorage.getItem("theme");
    if (valid.indexOf(t) === -1) {
      t =
        window.matchMedia &&
        window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light";
    }
    var el = document.documentElement;
    if (t !== "light") el.classList.add(t);
    el.style.colorScheme = t === "dark" || t === "graphite" ? "dark" : "light";
  } catch (e) {}
})();
