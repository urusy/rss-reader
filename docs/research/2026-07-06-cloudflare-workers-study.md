# rss-reader の Cloudflare Workers 移行検討

## 1. エグゼクティブサマリ

**結論: 移行するなら案C（Containers ハイブリッド・批評反映版）を推奨する。ただし移行そのものの合理性は条件付きであり、LAN フィード購読の継続・自宅からの体感速度・Rust 学習という本プロジェクトの目的価値を重視するなら「移行しない」が引き続き最有利である（§5）。**

- **案C を推奨する理由**: 3案中唯一、現行の技術品質を一切劣化させない。pg_trgm による日本語全文検索、web-push-native の暗号実装、「常駐1プロセス＝暗黙の排他」という並行性モデル、SSRF ガードの DNS 先行検査 — これらすべてが無修正で温存される。本検討は工数を考慮しない理想形の比較だが、案A の弱点（検索品質の縮退、D1 の行 2MB／DB 10GB 上限、対話的トランザクション不可）は**工数を投じても解決しない恒久的制約**であり、「工数不問」の前提でも消えない。
- **案B は不採用**: 温存度で案C に劣り（repository 層・HTTP 層・通知サイクル意味論の書き換えが必要）、クラウドネイティブ度で案A に劣る中間解でありながら、pre-1.0 の workers-rs と非公式な tokio-postgres/wasm という**3案中最大の成熟度リスク**を単独で抱える。外部 Postgres（Neon）が必要な点は案C と同じであり、案C に対する優位が実質的に存在しない。
- **案A は「Cloudflare ネイティブの理想形」としての参照価値**を持つ。運用コスト最小（約 $5〜7/月）・完全マネージドだが、検索・DB 制約・LAN 喪失という機能面の後退を受け入れる場合に限る。段階的移行パス（§6）の最終到達点候補として保持する。
- **月額**: 案C は常駐構成で **約 $25〜44/月 + LLM 従量**。Falcon ADR の結論どおりコスト削減にはならない。買っているのは自宅サーバ運用（電源・回線・OS 更新・バックアップ媒体）の消滅と常時 HTTPS である。

---

## 2. 前提整理

### 現行構成

| 層 | 実装 |
|---|---|
| API | Rust / Axum 0.8 / Tokio、Vertical Slice 24スライス（feeds/articles/search/notifications/auth/backup/digest/clustering/automation/read_later/Instapaper 連携ほか） |
| DB | PostgreSQL 17（compose 内部接続のみ）、sqlx 実行時クエリ、migration 0001〜0022 |
| 検索 | pg_trgm GIN インデックス + `ILIKE '%q%'` + `similarity()` ランキング（日本語に単語境界がなく tsvector 不採用の経緯あり） |
| スケジューラ | `shared/scheduler.rs` の tokio 常駐ループ（クロール15分毎 → mute 適用 → 通知、digest 毎時起床判定、clustering、pg_dump バックアップ） |
| Web Push | web-push-native（純Rust ECE/ES256）、watermark 送信前 commit の at-most-once、`DISPATCH_IN_FLIGHT`（AtomicBool）排他、Semaphore(8) 並行 |
| 認証 | Argon2id + HttpOnly Cookie セッション、プロセス内 LoginLimiter、CSRF は Origin==Host 検証 |
| セキュリティ | SSRF ガード（DNS 解決後 IP 検査 + `ALLOW_PRIVATE_NETWORKS` で LAN opt-in）、本文 10MB 上限 |
| 配信 | nginx（静的配信 + `/api` プロキシ + CSP、body 256MB） |
| バックアップ | pg_dump 子プロセス + ローカル FS、NDJSON export/import（256MB 一括 POST・単一 Tx） |

### 移植の論点

1. **常駐スケジューラ**: Workers に常駐プロセスは存在しない。Cron Triggers は at-most-once（リトライなし）。「クロール全完了 → ミュート適用 → 通知」の**順序保証**と、watermark の read-modify-write 排他をどう再現するかが核心。
2. **生 TCP Postgres**: Workers から sqlx は動かない。D1 に載せ替える（案A）、Hyperdrive + wasm ドライバ（案B）、コンテナから直結（案C）の三択。D1 は pg_trgm 相当を持たず、対話的トランザクション不可・行 2MB・DB 10GB 上限。
3. **pg_trgm 日本語検索**: D1/FTS5 trigram は近似にとどまり（3文字未満クエリ非対応・bm25 ランキングへの変質・FTS 仮想テーブル入り DB は export 不可）、無劣化の道は「Postgres を持ち続ける」以外にない。
4. **Web Push**: Node の web-push は Workers で動かない。WebCrypto ベース TS ライブラリへの乗り換え（案A）か、純Rust 実装の wasm 移植（案B）か、コンテナで無修正稼働（案C）。
5. **大容量バックアップ**: pg_dump 子プロセスとローカル FS は Workers に存在しない。エッジのボディ上限（Free/Pro 100MB）により 256MB 一括 import も不成立。R2 マルチパート + チャンク処理への再設計が全案共通で必須。

### 批評で浮上した横断論点（全案共通）

- **LAN フィード購読の喪失**: `ALLOW_PRIVATE_NETWORKS` はどの案でも意味を失う。継続するには Cloudflare Tunnel の**自宅常駐デーモン**が要り、「自宅運用の消滅」という移行便益の柱が半分崩れる。**移行可否判断の前に、LAN フィードを現に購読しているかの棚卸しが前提条件**。
- **体感レイテンシの退行**: 現行は自宅 LAN でサブ ms〜数 ms。移行後はエッジ→DB の多段往復で 50〜150ms 級になり、主に自宅から使う単一ユーザーには**改善ではなく劣化**。エッジ配信の恩恵は初回ロードのみ。
- **既存データの移行手順**: Postgres→D1 は型変換・25テーブル投入・FTS 再構築を要する大工事（案A）。Postgres→Neon は pg_dump/pg_restore で素直（案B/C）。
- **digest メール（email.rs スタブ）**: SMTP 生 TCP はどの案でも非現実的（Containers ですら port 25 遮断・587/465 未確認）。**Resend 等の HTTP API 送信に方針確定する。スタブ段階の今が切替好機**。
- **設定の二重管理**: `FEED_REFRESH_INTERVAL_SECS` / `DIGEST_HOUR_UTC` 等が env から wrangler の cron 式へ移り、実行時刻の変更が再デプロイ事項になる。
- **開発体験**: `just dev-db` + cargo watch のローカルループは全案で劣化する。1人開発では運用時間より開発イテレーションの方が支配的である点を判断材料に含める。

---

## 3. 3案の比較

| 観点 | 案A: フルネイティブ TS | 案B: workers-rs + Hyperdrive + Neon | 案C: Containers ハイブリッド |
|---|---|---|---|
| **概要** | Hono + D1 + Queues + DO へ全面書き換え | Rust を wasm 化し Neon へ Hyperdrive 接続 | 既存 Rust バイナリをコンテナ稼働、Worker は nginx 後継 |
| **技術適合** | △ 検索が FTS5 trigram に縮退（3文字未満クエリ非対応・ランキング変質）。D1 行 2MB／10GB 上限・対話 Tx 不可が恒久制約。web-push は TS ライブラリへ乗換 | ○ SQL・スキーマ・pg_trgm 無修正。ただし repository 層・HTTP 層は全面機械書換、通知サイクル意味論（per-feed 発火で watermark が新着を飲み込む）とミュート適用順序は**再設計必須**（設計案の「plan_cycle 無修正温存」は不成立） | ◎ sqlx・tokio・pg_trgm・web-push-native・SSRF DNS 検査・単一プロセス排他がすべて無修正。順序保証（crawl→mute→notify）は関数呼び出し順のまま |
| **制約リスク** | 中: trigram 索引膨張で 10GB キャップ接近、FTS 入り DB の export 不可、readability の CPU 上限接触、Cron at-most-once | **高**: workers-rs pre-1.0、tokio-postgres/wasm は非公式検証レベル（PoC 不合格で全崩壊）、axum の `!Send` 儀式、wasm 10MB サイズ枠、KV レートリミッタは結果整合性でレース | 中: Containers は GA 直後、コンテナ配置リージョンを明示固定する API がない（Neon との距離が制御不能）、1/4 vCPU で Argon2id が 2〜4 秒級に、eviction で LoginLimiter リセット |
| **運用性** | ◎ 全マネージド・可動部品最少・ベンダー1社 | ○ ただし障害ドメイン2社、wasm でデバッグ・観測が最劣化 | ○ 2社構成・コンテナログ/cron 失敗の可観測性は自前で補う必要 |
| **書き換え量（参考）** | 全面（Falcon ADR 比 3〜6ヶ月級） | 「最小」を標榜するが実際は中〜大（repository 24スライス + 通知再設計 + PoC） | 最小（スケジューラ外部化と backup 入出力の作り直しのみ） |
| **月額（Cloudflare + DB）** | **約 $5〜7** | **約 $10〜24**（Neon Free はコンピュート時間枠で初月から超過する公算が高く、$0 開始は成立しない） | **約 $25〜44**（常駐 basic + Neon 有料）。scale-to-zero の「$10」は duty 試算が甘く、現実には $15〜25 程度 |
| **既存データ移行** | 大工事（型変換・FTS 再構築） | pg_dump/restore で素直 | pg_dump/restore で素直 |
| **総評** | ネイティブ理想形だが品質後退が恒久 | 中間解。C に温存度で、A に純度で劣り、リスクだけ最大 | **技術的成立性が3案中最高。推奨** |

**判断**: 工数を除外すると、案A の弱みは「書き換え量」ではなく「D1 という土台の恒久制約」に集約され、案C の弱みは「月額」に集約される。理想形＝**機能・品質を落とさない形**と定義するなら、答えは案C である。案B は両案の悪い部分（外部 DB 依存 + 書き換え + 最大の成熟度リスク）を併せ持つため脱落。

---

## 4. 推奨アーキテクチャ詳細（案C 批評反映版）

### 4.1 構成図

```
                    ┌──────────────────── Cloudflare ─────────────────────────┐
                    │                                                          │
 Browser / PWA      │  ┌─ Worker「gateway」(単一オリジン = 旧 nginx の3役) ──┐ │
 (SolidJS SPA,      │  │ [Static Assets] dist/ + SPA fallback                │ │
  sw.js, Push購読)  │  │   index.html: no-cache / ハッシュ資産: immutable    │ │
   │ HTTPS          │  │   を _headers 相当で明示（既定挙動に依存しない）     │ │
   ├───────────────▶│  │ [fetch] run_worker_first=["/api/*"]                 │ │
   │                │  │   → CSP 等ヘッダ付与 → DO へ転送（Host/Origin透過） │ │
   │                │  │   ※ /internal/* は絶対に転送しない（allowlist方式）│ │
   │                │  └──────────────┬──────────────────────────────────────┘ │
   │                │                 ▼                                        │
   │                │  ┌─ Durable Object「AppContainer」(固定ID シングルトン)─┐ │
   │                │  │  ┌─ Container: 既存 Docker イメージほぼそのまま ──┐ │ │
   │                │  │  │  Rust/Axum 24スライス (/api/*)                 │ │ │
   │                │  │  │  /internal/jobs/* (内部トークン必須, cron専用) │ │ │
   │                │  │  │  instance: basic (1/4 vCPU, 1GiB, 4GB)         │ │ │
   │                │  │  └──┬──────────┬──────────┬──────────┬────────────┘ │ │
   │                │  └─────┼──────────┼──────────┼──────────┼──────────────┘ │
 [Cron Triggers] ───┼─▶ scheduled() ────┘          │          │                │
  */15 crawl        │        │               ┌─────▼────┐     │                │
  0 H digest        │        │               │    R2    │◀────┘ pg_dump/NDJSON │
  0 */6 cluster     │        │               │ (backup) │   (ストリーム書出し) │
  0 3 backup        │  [AI Gateway]◀─────────┴──────────┘                      │
                    └────────┼──────────┬─────────────┬───────────────┬────────┘
                             ▼          ▼             ▼               ▼
                       Anthropic    Neon Postgres  フィード各サイト  Push 配信網
                       Messages API (sqlx TLS直結,  (safe_get の     (FCM/APNs/
                                     pg_trgm/PITR)   DNS検査そのまま)  Mozilla)

  (LANフィード継続時のみ) 自宅 ◀─ Cloudflare Tunnel ─▶ Container からの購読
```

### 4.2 コンポーネント対応（要点）

| 現行 | 移行先 | 備考 |
|---|---|---|
| nginx 3役 | gateway Worker（Static Assets + ヘッダ + 転送）。Host/Origin はポート込みで透過し CSRF 検証を無修正維持。**転送は `/api/*` の allowlist 方式とし、`/internal/*` 露出をルーティングミス一発で起こさない** | 設定移植 |
| Axum 24スライス | Container 内で無修正稼働（wasm 化しない。`!Send` 問題なし） | ほぼゼロ |
| sqlx + migrate | DATABASE_URL を Neon へ。migration 0001〜0022・`_sqlx_migrations` 履歴・ON CONFLICT・JSONB・RETURNING すべて無修正 | 接続先変更のみ |
| pg_trgm 検索 / clustering | Neon で `CREATE EXTENSION pg_trgm`。無修正（§4.3） | ゼロ |
| tokio 常駐3ループ | Cron Triggers → `/internal/jobs/*`（§4.3）。**唯一の実質的再設計点** | 中規模 |
| web-push-native / watermark / DispatchGuard | Container 内で無修正（§4.3） | ゼロ |
| SSRF ガード | Container は通常の Linux ネットワーク+DNS を持つため `lookup_host` 先行検査が**縮退なしで**動く（案A/B は縮退移植） | ゼロ |
| backup / import | Neon PITR を一次 + pg_dump→R2。**export/import 双方**をストリーミング化（§4.3） | 作り直し |
| 認証 | Container 内で無修正。ただし 1/4 vCPU での Argon2id 遅延と Cloudflare Access 代替を評価（§4.4） | 要調整 |
| digest メール | Resend 等 HTTP API に確定（SMTP 不採用） | 方針決定 |
| .env | Workers Secrets + Container env。`COOKIE_SECURE` 分岐削除（常時 HTTPS） | 設定移植 |
| SolidJS SPA / sw.js / manifest | 無修正 | ゼロ |

### 4.3 難所ごとの解法（批評反映済み）

**クロール／スケジューラ**
- `SCHEDULER_MODE=external` で常駐ループを止め、`/internal/jobs/refresh|digest|cluster|backup` を新設。中身は既存の `refresh_all_feeds` → `mute_rules::apply_all` → 通知起動を**同一関数呼び出し順で**呼ぶだけ。案A/B が構造的に壊す「クロール全完了→ミュート→通知」の順序保証が、案C では自明に保たれる。
- `/internal/jobs/*` は **Secrets 管理の内部トークン（Bearer）を必須化**。gateway の転送 allowlist と二重防御にする（批評指摘: 無認証ジョブ起動の露出リスク）。
- ジョブは 202 即返し + コンテナ内 `tokio::spawn`。**常駐構成（sleepAfter 20m ＋ 15分毎 cron）では DO の 1分毎 status ポーリングは組み込まない**（批評指摘: 主構成では不要な複雑さ）。scale-to-zero 構成に切り替える場合にのみ有効化するフラグとする。
- 排他はシングルトン DO で「コンテナ高々1個」を構造保証しつつ、退避・再起動またぎの二重実行に備え **`pg_advisory_lock` をジョブ入口に追加**（数行）。
- Cron は at-most-once（リトライなし）。`ensure_today` 等の冪等設計 + 次サイクル回収で吸収。**cron 失敗の検知は現行 `docker logs` より弱くなるため、外形監視（feed_health の最終成功時刻を返すヘルスエンドポイント + 無料の外部監視）を移行スコープに含める**（批評指摘: 「気づいたらクロールが3日止まっていた」の防止）。
- **条件付き GET（ETag/If-Modified-Since）を新設**。従量課金環境ではコスト防御として優先度が上がり、Neon のコンピュート消費も抑える。

**検索**
- pg_trgm を Neon でそのまま。migration 0005・`ILIKE + similarity()`・clustering の自己結合・`make_interval()` まで SQL 一行も変えない。検索品質は現行と完全同一。O(n²) クラスタリングもコンテナ→Neon 直結なので Hyperdrive のタイムアウト問題（案B の未解決点）と無縁。
- 注意はレイテンシのみ: **コンテナの配置リージョンは明示固定できない**ため、Neon（AWS ap-southeast-1 等）との RTT は**移行前の PoC で実測必須**の検証項目とする。DO locationHint(apac) は寄せる努力であって保証ではない。

**Web Push**
- web-push-native・watermark の送信前 commit（at-most-once）・404/410 購読 GC・Semaphore(8) 並行 — すべて無修正。シングルトンコンテナで `DISPATCH_IN_FLIGHT` の前提が回復する。
- 例外を1つ明記: **LoginLimiter（プロセス内メモリ）はコンテナ退避のたびにリセット**される。攻撃者が意図的に退避を誘発できないため実害は小さいが、「ほぼゼロ変更」の例外として認識し、恒久対処は §4.4 の Access 移行か DB カウンタ化で行う。

**バックアップ**
- 一次: **Neon PITR**（プランにより保持 24h〜7日。世代保持の長期分は pg_dump が担う、という使い分けを明文化）。
- 二次: cron → コンテナ内 pg_dump → **R2 へ S3 API でマルチパートアップロード**。ディスクはエフェメラルなのでローカル保存は廃止。
- **NDJSON export は「全件を単一 String に連結」の現行実装のままでは basic の RAM 1GiB が新たな天井になる**（批評指摘: 自宅サーバより低い）。export・import の**双方**をストリーミング化する。import はエッジ body 上限 100MB のため「R2 マルチパート直行 → コンテナが R2 から pull してチャンク投入」方式へ。
- 最単純の保険として、**Neon は素の Postgres なので手元マシンから pg_dump を直接叩ける**ことを運用手順に明記する（案B 批評で指摘された、自前実装ゼロの脱出口）。

**AI（要約・翻訳・ask・digest）**
- `shared/llm` の baseURL を AI Gateway に向けるだけ（1行）。ただし**価値はログとレート制限に限定**される — リクエストボディ完全一致キャッシュは記事本文込みプロンプトにはヒットせず、アプリの DB キャッシュが既に一次防壁（批評指摘を明記）。
- LLM 待ちはコンテナ内の通常 I/O であり、Workers の CPU 課金・壁時計制限と無縁。非ストリーミングの現行 `LlmClient` trait を**変更せずに**移行できる（案B で露呈した「SSE 化が trait 全面に波及する」問題を回避）。SSE 化は移行後の独立した改善課題として切り離す。
- relevance scoring・automation_rules・cluster_summary は `fetch_and_store` / clustering ジョブ内の現行位置のまま動く（コード無修正）。ただしこれらは記事単位の LLM 呼び出しを含むため、**Anthropic 費用の見積りにはオンデマンド分と別枠で計上**する（案A 批評指摘の轍を踏まない）。

### 4.4 認証の再考（批評反映）

1/4 vCPU では Argon2id 検証が 2〜4 秒級に伸びうる（lite なら 10 秒級で実用性が怪しい）。対処は二択:

- **(a) 現行スタック維持 + パラメータ調整**: ログインは低頻度なので 2〜3 秒は許容可能。m_cost/t_cost を実測で調整。LoginLimiter は DB カウンタ化して eviction 耐性を持たせる。
- **(b) Cloudflare Access（50ユーザーまで無料）を前段に**: 単一ユーザーなら Argon2・セッションテーブル・LoginLimiter・CSRF の大半を Access が肩代わりでき、eviction 問題も消える。**理想形としてはこちらが単純**だが、直近で実装したばかりの自前認証（migration 0022）を捨てる判断になる。移行時に比較検討する事項として明記（案A/C 両批評が指摘した「検討痕跡がない」の解消）。

### 4.5 既存データ移行と設定管理

- **データ**: 手元から `pg_dump | pg_restore` で Neon へ投入 → `CREATE EXTENSION pg_trgm` → sqlx migrate の履歴整合を確認。D1 案と違い型変換ゼロ・FTS 再構築ゼロ。移行リハーサルを本切替前に1回実施。
- **設定分裂**: `DIGEST_HOUR_UTC` 等が cron 式（wrangler.toml）へ移り、変更が再デプロイ事項になる。cron 式は「毎時実行 + アプリ側で時刻判定」の現行ロジックを残す選択肢もあるが、単純さを優先し cron 式一本化 + 「時刻変更 = デプロイ」を運用ルールとして受容する。

### 4.6 月額コスト（批評反映版）

| 項目 | 内容 | 月額 |
|---|---|---|
| Workers Paid | 基本料（Containers/DO/Cron 込み） | $5.00 |
| Container basic 常駐（730h） | メモリ ≈$6.3 + CPU ≈$12.7 + ディスク ≈$0.7 | ≈$20 |
| Container egress | APAC 単価高めだが個人規模では数十セント級（**項目として計上**） | <$1 |
| Neon | **$0 開始は成立しない** — 15分毎クロールで autosuspend がほぼ効かず、コンピュート時間枠を超過。Launch 級を初月から前提 | $19 |
| R2 / Static Assets / Cron / AI Gateway | 無料枠内 | $0 |
| **Cloudflare + DB 合計** | | **≈$25〜44** |
| Anthropic API | オンデマンド + digest + relevance/automation/cluster_summary の自動呼び出し分 | 数$〜（利用量次第・自宅でも同額） |

scale-to-zero 切り詰めは「クロール数分 + sleepAfter」で duty が 50〜90% に張り付くため、**$10 まで落ちるという期待は持たない**（$15〜25 が現実線）。コールドスタート実測（イメージ pull + 起動 + migrate）も PoC 項目。

---

## 5. セルフホストとの本質的トレードオフ

| 軸 | セルフホスト（現行） | Cloudflare 移行（案C） |
|---|---|---|
| **データ主権・プライバシー** | 記事・閲覧履歴・LLM キャッシュ・認証情報が自宅から出ない（LLM 入力のみ Anthropic へ） | 全データが Cloudflare + Neon の2社に置かれる。購読フィードと閲覧行動は嗜好のプロファイルそのもの |
| **コスト** | 電気代のみ（≈$0） | $25〜44/月の恒常固定費。**コスト削減には決してならない**（Falcon ADR と同型の結論） |
| **可用性** | 停電・回線断・ハード故障が単一障害点。外出先アクセスは Tunnel 等の追加運用 | マネージド。常時 HTTPS・外出先アクセスが標準。ただし cron 失敗検知・コンテナログ追跡は自前補完が必要で、可観測性は `docker logs` より当初劣る |
| **レイテンシ** | 自宅 LAN サブ ms | edge→DO→Container→Neon の多段で 50〜150ms+。**主に自宅から使うなら明確な劣化** |
| **運用負荷** | OS 更新・compose 再起動・ディスク監視・バックアップ媒体 | ほぼ消滅（LAN フィード継続なら Tunnel デーモンで一部残存） |
| **開発体験** | `just dev-db` + cargo watch の高速ループ | wrangler + Containers のローカル開発は未成熟。イテレーション劣化 |
| **資産** | Rust 学習という明示目的に直結 | 案C はコード温存で毀損最小（案A なら放棄） |

### 「移行しない」判断が合理的になる条件

以下のいずれかに該当するなら、移行しない（または §6 の Phase 1〜2 で止める）ことが合理的である:

1. **LAN 内フィードを現に購読している** — 移行しても Tunnel 常駐で自宅運用が残り、移行便益の柱が崩れる。
2. **利用が主に自宅・単一ユーザーで、体感速度を重視する** — 移行は UX を悪化させる方向にしか働かない。
3. **Rust を実コードで学ぶことがプロジェクトの目的である** — 運用消滅と引き換えに学習環境（ローカルループ・観測性）を差し出すのは目的に反する。
4. **月額固定費 $25+ を「自宅サーバの保守時間」より高く評価しない** — 損益分岐は電気代 + 保守時間 + 外出先アクセス需要の総和との比較であり、外部公開の需要がなければ分岐点に届かない。

逆に移行が効くのは、**外出先・複数デバイスからの常用が主で、自宅にサーバを置き続けること自体を止めたい**場合である。Falcon ADR の結論（セルフホスト継続 or IaaS 移設が最有利）を覆す新事実は本検討でも現れていない — なお「書き換え最小」の比較では、現行 compose を無改修で動かせる IaaS（Hetzner €4〜8 等）が案C より単純である点も判断材料として付記する。

---

## 6. 段階的移行パス（参考）

理想形（§4）へ一足飛びに行かず、各段階で可逆性を保つ。

- **Phase 0 — 事前確定（コード変更なし）**
  LAN フィードの棚卸し（移行可否の前提条件）／DB 実データ量と Neon プラン試算／digest メールを HTTP API（Resend 等）に方針確定／コンテナ⇔Neon RTT・コールドスタートの PoC 実測。
- **Phase 1 — DB だけ Neon へ**（自宅 compose は継続）
  DATABASE_URL 切替のみ。pg_trgm・migration 無修正で、外部 DB 化のレイテンシ影響を**現構成のまま**実測できる。合わなければ戻すだけ（完全可逆）。バックアップに Neon PITR が加わる。
- **Phase 2 — フロントを Static Assets へ、API は自宅 + Tunnel**
  gateway Worker（静的配信 + `/api` を Tunnel 経由で自宅 Axum へ転送）。常時 HTTPS・エッジ配信・外出先アクセスをこの時点で獲得。単一オリジン維持で Cookie/CSRF 無修正。**多くのユースケースではここが費用対効果の頂点**であり、ここで止める判断も正当。
- **Phase 3 — 本移行（案C）**
  Container 化・スケジューラ外部化（cron + `/internal/jobs/*` + 内部トークン + advisory lock）・backup の R2 化とストリーミング化・外形監視の整備。自宅サーバ停止（LAN フィードを放棄した場合のみ完全停止）。
- **Phase 4 —（任意）ネイティブ化の漸進**
  安定運用後、ホットパス（検索・一覧 API）を Workers ネイティブへ切り出す、認証を Cloudflare Access へ寄せる等、案A 方向への部分接近を個別に評価。全面 TS 化は D1 制約が解けない限り目標としない。

各 Phase は独立にロールバック可能であり、「途中で挫折した場合の出口」（二重運用の解消・自宅復帰）が常に一手で確保される。
