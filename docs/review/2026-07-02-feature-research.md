# RSS リーダー機能リサーチ(2026-07-02)

- **方法**: 並行リサーチエージェント2体による Web 調査(2025-2026 の最新リリース・公式ドキュメント)。OSS: Miniflux 2.2/2.3、FreshRSS 1.26、CommaFeed 6/7、Tiny Tiny RSS、NewsBlur、glance。商用/サービス: Feedly、Inoreader、Readwise Reader、Matter、Reeder 新版、NetNewsWire、Folo、Particle、Ground News、RSS.app。
- **前回調査(2026-06-30、外部110機能)との関係**: 前回成果は docs/design/12-34 の設計書群として結実し、**設計 13〜28・31〜33 は実装済み**(全文抽出/認証/バックアップ/Read-on-Save/OPML/キーボードナビ/ミュート/フィード自動検出/フィード健全性/Ask Claude/デイリーダイジェスト/タグ+自動タグ/関連度スコア/クラスタリング/スマートビュー/ルールエンジン/Web Push/スター・ハイライト/TTS)。未実装は 29 GReader/Fever 同期API(見送り)・30 ニュースレター→RSS・digest のメール送信(スタブ)のみ。本調査は**その先の取りこぼしと 2025-2026 の新潮流**にフォーカスした。

---

## 1. トップピック(価値/コスト比で推奨)

| # | 機能 | 由来 | 区分 | コスト感 |
|---|------|------|------|----------|
| 1 | 条件付き GET(ETag/If-Modified-Since) | Miniflux ほか | クローラ | 小 |
| 2 | アダプティブ・ポーリング(per-feed 取得間隔) | 一般 | クローラ | 中(apalis 移行と同時) |
| 3 | 読了時間の推定表示 | ほぼ全リーダー | 読書UX | 極小 |
| 4 | テキスト選択アクション(選択→質問/翻訳/かみ砕き) | Readwise Ghostreader / Matter | AI | 小〜中 |
| 5 | クロス記事レポート(複数記事を選んで一括分析) | Inoreader Intelligence | AI | 中 |
| 6 | 一覧の見出し一括翻訳 | Folo | AI/UX | 小 |
| 7 | 記事保持ポリシー(自動パージ) | Miniflux 2.3 | 運用 | 小 |
| 8 | コマンドパレット(Cmd+K) | Readwise Reader | 読書UX | 小(キーボードナビ実装済みが土台) |
| 9 | ハイライト→Markdown/Obsidian エクスポート | Matter / Readwise | 読書UX | 小(annotations 実装済みが土台) |
| 10 | AI デイリー音声ブリーフ(台本化→TTS) | Particle / WaPo | AI | 中(digest+TTS 実装済みが土台) |

---

## 2. クローラ・フィード取得の改善(OSS 由来)

apalis 移行(着手中・方式B)と同じ領域。移行設計に織り込むと二度手間がない。

### 2-1. 条件付き GET(ETag / If-Modified-Since / 304) — 最優先
出典: Miniflux(https://miniflux.app/features.html)、openrss.org ガイド
`ETag`/`Last-Modified` を保存し `If-None-Match`/`If-Modified-Since` を送る。未更新なら 304 空ボディで再ダウンロード・再パース・DB 書込を丸ごと省略。現行クローラは毎サイクル全文取得しており、帯域・CPU を最も安く削減できる第一手。feeds テーブルにカラム2本+ヘッダ処理のみ。

### 2-2. アダプティブ・ポーリング
出典: 一般クローラ設計(nikolajjsj.com/blog/building-an-intelligent-rss-feed-fetcher)
`<ttl>`・`<sy:updatePeriod>`・過去の更新頻度から per-feed の次回取得時刻を決める。動かないフィードを間引き、速いフィードは頻繁に。apalis の per-feed ジョブスケジュールと設計が噛み合う。

### 2-3. per-host レート制御+識別可能な User-Agent
出典: brntn.me/blog/respectfully-requesting-rss-feeds
同一ホストへの同時接続・頻度を絞り、連絡先入り UA を送る「行儀の良い」クローラ化。ブロック/BAN 回避。SSRF 対策のリダイレクト検証と一体で設計できる。

### 2-4. 更新済みエントリの扱い(ignore_entry_updates)
出典: Miniflux 2.2
既存エントリが更新されたとき上書きするか無視するかを per-feed で選択。**既読が勝手に戻る事故の予防**で、既読管理を作り込んでいる本アプリでは実務価値が高い。

### 2-5. WebSub(PubSubHubbub)サブスクライバ
出典: FreshRSS(freshrss.github.io/FreshRSS/en/users/WebSub.html)
`<link rel="hub">` を検出して購読登録し、WordPress/Mastodon/Medium 等からは新着を push で即時受信。実装済みの優先度フィード Web Push と組み合わせると「対応元は即時通知」が完成する。エンドポイント1本+購読管理テーブル。

### 2-6. Media RSS / サムネイル活用
出典: Miniflux
`media:content`/`media:thumbnail` を取り出して一覧カードに表示。視認性向上と 3-4(メディア対応)の下地。

---

## 3. AI 機能(商用由来 — Claude API 課金済みという優位を活かす)

### 3-1. テキスト選択アクションメニュー ★
出典: Readwise Ghostreader(docs.readwise.io/reader/guides/ghostreader/overview)、Matter Co-Reader
選択した語句・段落に「意味を調べる/用語解説/その場翻訳/平易化」をポップアップから即実行。sanitize 済み本文にセレクション監視+定型プロンプトで Claude 呼び出し。単発・低トークンで体験価値が大きい。Matter は選択時のプリフェッチ(先行リクエスト)で体感遅延を消している。

### 3-2. クロス記事インテリジェンスレポート ★
出典: Inoreader Intelligence(2025-12 発表)
複数記事を選択して1プロンプトで一括分析(要点比較・論調・センチメント)。「今日の未読5本を比較して」。Claude の長コンテキストが強みそのまま活きる。html_to_plain_text(実装済み)で平文化して束ねる新スライス1枚。

### 3-3. 多視点クラスタサマリ(dedup→クラスタ→各社の立場比較)
出典: Particle、Ground News
同一事件の複数記事を1ストーリーに束ね「共通点/相違点/各社の論調」を提示。**clustering スライス実装済み**なので、クラスタ内本文を Claude に渡す要約プロンプトの追加が主作業。バイアスメーターまでやるかは好み。

### 3-4. AI デイリー音声ブリーフ(ラジオ台本→TTS)
出典: Particle 音声フィード、Washington Post AI podcast(2025-12)
1日分のダイジェストを Claude でラジオ台本化し TTS で連続再生。**digest(実装済み)+TTS 進捗永続化(実装済み)の合流点**で、通勤中のハンズフリー消化という新しい利用シーンを開く。あわせて「未読キューの連続再生(フィードを再生)」も同基盤。

### 3-5. 読解レベル調整(平易化/ELI5/専門家向け)
出典: Ghostreader
要約/翻訳と同じ経路にプロンプト差し替えで追加できる。llm_settings(実装済み)のプロンプト個別指定がそのまま使える最小コスト AI 機能。

### 3-6. 一覧の見出し+要旨の一括翻訳
出典: Folo(folo.is)
本文を開く前に一覧のタイトル/要旨を母語で表示。外国語フィードの拾い読みが激変する。短文バッチなので低コスト。既存翻訳キャッシュの一覧適用版。

### 3-7. ルールエンジンへの AI アクション接続
出典: Inoreader Rules(2025: Create summary/Translate を rule アクション化)
**automation_rules(実装済み)**のアクションに「自動要約/自動翻訳/自動タグ」を追加し、オンデマンド AI を条件付き自動化に昇格。コスト暴走防止のため対象を絞る条件(優先度フィードのみ等)とセットで。

### 3-8. AI トレンド/新興トピック検出
出典: Feedly Leo(business events)
期間内の記事群から「急に立ち上がった話題」を抽出し Web Push(実装済み)で通知。digest の姉妹機能。

### 3-9. センチメント・固有表現の構造化抽出タグ
出典: Inoreader Intelligence
論調・企業・人物・製品を Claude の構造化出力(JSON)で抽出しタグ化。tags+自動タグ(実装済み)の意味リッチ化。

### 3-10. AI 自動ミュート(意味ベースのノイズ除去)
出典: Feedly Leo mute filter、NewsBlur Intelligence Trainer
キーワードでなく「意味的に興味外」を判定して畳む。relevance(実装済み)+mute_rules(実装済み)の統合先。NewsBlur の 2026 年改修(URL 分類器・regex・一元管理タブ)は UI の参考になる。

### 3-11. プロンプトライブラリ
出典: Inoreader BYOAI(2026-04)、Ghostreader カスタムプロンプト
定型プロンプトをユーザー登録して呼び出す。llm_settings(実装済み)の自然な拡張。用途別モデル選択(haiku=軽処理/sonnet=要約)はコスト最適化にもなる。

---

## 4. 読書体験(UX)

### 4-1. 読了時間の推定表示 ★
出典: ほぼ全リーダー標準。文字数から算出するだけのフロント単独軽実装で、「読むか・要約で済ますか」の判断材料になる(要約機能との相乗)。

### 4-2. コマンドパレット(Cmd+K)
出典: Readwise Reader。キーボードナビ(実装済み)を土台に、全アクション検索実行の薄いレイヤを載せる。以後の機能追加の受け皿にもなる。

### 4-3. ハイライト+メモの外部エクスポート(Markdown/Obsidian/Notion)
出典: Matter、Readwise。annotations(実装済み)を「読書の知識ベース化」まで価値化する仕上げ。Markdown/JSON エクスポート1本から。

### 4-4. タイポグラフィ設定(フォント/サイズ/行間/幅)
出典: Readwise Reader、NetNewsWire。prose に CSS 変数上書き+localStorage。フロント単独。

### 4-5. スワイプ・トリアージジェスチャ
出典: Readwise(カスタム割当)、Miniflux。一覧の左右スワイプに既読/後で読む/スターを割当。pointer-coarse 対応(実装済み)の次の一歩。

### 4-6. タイムライン位置同期・未読カウント非表示モード
出典: Reeder 新版。「全部読む」強迫からの解放という思想。未読バッジ非表示オプション+読み位置の永続化だけなら小さい。

### 4-7. 記事の並び替え(公開日/ランダム)
出典: FreshRSS 1.26。積読消化のランダム表示など、一覧クエリの ORDER BY 追加程度。

### 4-8. 引用→画像カード生成
出典: Matter。ハイライトを共有用画像に。Canvas/SVG+デザイントークンで Claude 不要。優先度低め。

---

## 5. コンテンツソース拡張

### 5-1. YouTube / Podcast 対応(インライン再生+エンクロージャプレイヤー)
出典: Miniflux(youtube-nocookie)、Inoreader。YouTube はフィード URL 変換だけで購読でき、エンクロージャの音声はプレイヤー追加で Podcast 化。TTS の再生位置永続化ノウハウが横展開できる。文字起こし→要約(Inoreader 2025)は whisper 系が別途要るため後回しでよい。

### 5-2. 任意 Web ページのフィード化・変更監視
出典: Feedly Monitor、RSS.app、FreshRSS XPath scraping(1.26 で JSON-LD 対応)
RSS を持たないサイトを XPath/CSS セレクタで疑似フィード化 or 差分監視。extraction(実装済み)とスケジューラの合流点。上級機能だがセルフホストの醍醐味。

### 5-3. 任意 URL/PDF の保存(何でも後で読む)
出典: Readwise Reader。URL 保存→全文抽出(実装済み)→記事モデルへ正規化で「Instapaper の内製版」。read_later(実装済み)の発展形。

### 5-4. Reddit/Bluesky/Mastodon アダプタ
出典: Reeder 新版、Inoreader。プラットフォーム別アダプタを縦割りスライスで追加。需要が出てからで可。

### 5-5. タグ/フォルダの公開フィード書き出し
出典: Reeder(public JSON feed)。キュレーション共有。auth(実装済み)と絡めた限定公開も。

---

## 6. 整理・トリアージ

### 6-1. 優先度受信箱(スコア降順の既定ビュー)
出典: Feedly Priority、Particle。relevance(実装済み)+saved_views(実装済み)の組み合わせで「上から読めば重要分だけ拾える」タブを作る。Web Push の優先度フィード判定とも一貫。

### 6-2. 重複記事の検出・集約(dedup)
出典: Feedly Leo、Ground News。clustering(実装済み)の日常運用版 — 一覧で重複を1件に畳む表示。境界判定のみ Claude の二段構え。

### 6-3. keep 型フィルタ(ホワイトリスト)と per-feed フィルタ
出典: Miniflux rules、CommaFeed 7(CEL+ビジュアルビルダ)
mute_rules/automation_rules(実装済み)は block 型中心。「該当のみ残す keep」「global と per-feed の二層」を足すと Miniflux 相当の表現力に。CommaFeed 7 の CEL 採用(JEXL からの置換)はサンドボックス性の好例。

### 6-4. ルールスコアリング(加点/減点)
出典: Tiny Tiny RSS。ルールに符号付きスコアを持たせ並び順に反映。AI 関連度と併存する「決定的で説明可能な」軸。

---

## 7. 運用・セルフホスト(小粒)

- **記事保持ポリシー(自動パージ)** ★ — Miniflux 2.3(上限1000件+tombstone で削除済み未読の復活防止)。無期限蓄積の現状に「既読 N 日で削除・フィード当たり上限」を導入。**要約/翻訳キャッシュ(=消費トークン)を消さない設計**を忘れずに。
- **favicon 自動取得** — FreshRSS 1.26。サイドバーの識別性向上。取得は SSRF ガード配下で。
- **ntfy/Gotify 通知先追加** — CommaFeed 7。Web Push(実装済み)の通知先を抽象化するだけ。セルフホスト勢の定番。
- **アウトバウンド Webhook** — Miniflux。新着イベントを任意 URL へ POST、Home Assistant 等と接続。スライス1枚。
- **Prometheus /metrics** — Miniflux。取得成功率・LLM トークン・キュー長。apalis 移行後の可視化に(技術負債レビュー B-2/D-6 と同根)。
- **共有連携の拡充(Wallabag/linkding/Telegram)** — Miniflux は25+連携。Instapaper(実装済み)の横に数個足す。
- **メディア/画像プロキシ** — Miniflux。トラッキング排除+mixed content 回避。SSRF ガードと同じ基盤を共有。
- **パスキー(WebAuthn)ログイン** — Miniflux 2.2/2.3。auth(実装済み・トークン)を iPhone/iPad の生体認証で摩擦レスに。
- **購読ブックマークレット / PWA share target** — feed_discovery(実装済み)への入口 UX。モバイル1タップ購読。
- **カスタム CSS 注入口** — Miniflux。テーマ4種+トークン基盤の上に1テキストエリア。JS 注入は XSS 面から不採用推奨。

---

## 8. 周辺情報(調査時の環境認識)

- Tiny Tiny RSS はオリジナル開発が 2025 年に縮小気味でフォーク中心。参考にはするが追従先ではない。
- CommaFeed 7(2026): CEL フィルタ・push 通知・SSRF 既定ブロックが目玉 — 本アプリの SSRF 対応(技術負債 A-1)の実装参考。
- Miniflux 2.3.0(2026-05): OPML 設定込みエクスポート・passkey・tombstone。
- FreshRSS 1.26: ソート・favicon・WebSub・XPath スクレイピング強化。
- 商用の潮流は「単記事要約 → 複数記事の統合知能(比較/クラスタ/音声化/自動化)」へ。**本アプリは digest/clustering/relevance/rules/TTS を既に持っており、これらを「接続」するだけで商用最前線に並べる位置にいる。**

## 主要出典

- Miniflux: https://miniflux.app/features.html / https://miniflux.app/docs/rules.html / https://miniflux.app/releases/2.3.0.html / https://github.com/miniflux/v2/releases
- FreshRSS: https://github.com/FreshRSS/FreshRSS/releases/tag/1.26.0 / https://freshrss.github.io/FreshRSS/en/users/WebSub.html
- CommaFeed: https://github.com/Athou/commafeed/blob/master/CHANGELOG.md
- NewsBlur: https://blog.newsblur.com/2026/01/22/intelligence-trainer-overhaul/ / https://www.newsblur.com/features
- TT-RSS: https://tt-rss.org/docs/Features.html
- Inoreader: https://www.inoreader.com/blog/2025/12/inoreader-2025-intelligence-and-automation-in-one-content-hub.html / https://ppc.land/inoreader-launches-enhanced-podcast-features-with-ai-powered-transcripts/
- Feedly: https://feedly.com/ai / https://feedly.com/new-features/posts/the-monitor-tracking-the-known-present
- Readwise Reader: https://readwise.io/read / https://docs.readwise.io/reader/guides/ghostreader/overview
- Matter: https://www.getmatter.com/
- Reeder: https://reederapp.com/ / https://www.macstories.net/reviews/reeder-a-new-approach-to-following-feeds/
- Folo: https://folo.is/ / https://github.com/RSSNext/Folo
- Particle: https://particle.news/ / https://techcrunch.com/2026/02/23/particles-ai-news-app-listens-to-podcasts-for-interesting-clips-so-you-you-dont-have-to/
- Ground News: https://ground.news/
- クローラ実装: https://nikolajjsj.com/blog/building-an-intelligent-rss-feed-fetcher/ / https://brntn.me/blog/respectfully-requesting-rss-feeds/ / https://openrss.org/guides/developers-guide-to-open-rss-feeds

---

*生成: 2026-07-02、リサーチエージェント: research-oss / research-saas。実装状況の突合は backend/src/features/(24スライス)・migrations 0001-0020・git log に基づく。*
