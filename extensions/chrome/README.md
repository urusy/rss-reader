# RSS Reader — 後で読む（Chrome 拡張）

表示中のページを自分の RSS Reader に保存する Manifest V3 拡張。ビルド不要のプレーン JS。

## セットアップ

1. サーバーの `.env` に `SAVE_TOKEN=<十分長いランダム文字列>` を設定して再起動する
   （未設定だと `/api/save` 自体が存在しない = 404）。
2. Chrome で `chrome://extensions` を開き「デベロッパー モード」を ON。
3. 「パッケージ化されていない拡張機能を読み込む」→ この `extensions/chrome/` ディレクトリを選択。
4. 拡張の「オプション」でサーバー URL（例 `http://192.168.0.8:8080`）と SAVE_TOKEN を保存
   （このときサーバーへのアクセス権限ダイアログを許可する）。

## 使い方

保存したいページでツールバーのアイコンをクリック。バッジで結果を表示する:

- `✓` 保存成功（リーダーの「後で読む」に入り、本文は背景で抽出される）
- `401` トークン不一致 / `!` 未設定・通信失敗 / `×` http(s) 以外のページ

## 仕組みのメモ

- 認証は `Authorization: Bearer <SAVE_TOKEN>`（サーバーは constant_time_eq で照合）。
- MV3 service worker からの fetch はホスト権限があれば CORS 検査を受けないため、
  サーバー側の CORS 設定変更は不要。
- トークンは `chrome.storage.local` に保存（`sync` は使わない — Google 経由で同期され秘匿情報が外に出るため）。
