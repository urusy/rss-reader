//! 機能利用イベントの送信口（sink）。
//!
//! ここはチャネルの送信端だけを持ち、テーブル知識・INSERT は一切持たない。
//! 受信して DB に書く writer は `features/usage/service.rs` が所有する
//! （shared → features へ依存を張れないための依存反転）。
//!
//! `record` は非 async・非ブロッキング（unbounded send のみ）なので、
//! HTTP 応答パスやスケジューラから呼んでも遅延を持ち込まない。
//! sink 未 install（ユニットテスト等）なら no-op。テレメトリは失ってよい。

use std::sync::OnceLock;

use tokio::sync::mpsc::UnboundedSender;

/// 利用イベント。writer（features/usage）が受信して DB に書く。
#[derive(Debug)]
pub enum UsageEvent {
    /// HTTP ミドルウェア由来（status は応答ステータス）。
    Server { feature: &'static str, status: u16 },
    /// クライアント申告（feature/meta は handler で許可リスト検証済み）。
    Client {
        feature: String,
        meta: Option<serde_json::Value>,
    },
    /// LLM 実呼び出し（anthropic.rs の合流点由来）。
    Llm {
        purpose: &'static str,
        model: String,
        input_tokens: i64,
        output_tokens: i64,
    },
}

static SINK: OnceLock<UnboundedSender<UsageEvent>> = OnceLock::new();

/// 起動時に usage スライスが1回だけ呼ぶ。2回目以降は無視される。
pub fn install(tx: UnboundedSender<UsageEvent>) {
    let _ = SINK.set(tx);
}

/// fire-and-forget。未 install なら no-op、受信側が落ちていても無視。
pub fn record(ev: UsageEvent) {
    if let Some(tx) = SINK.get() {
        let _ = tx.send(ev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // OnceLock はプロセス内で共有されるため、install の有無両方を検証するには
    // 「未 install でも panic しない」→「install 後は届く」を単一テストで順に確認する。
    #[tokio::test]
    async fn record_is_noop_before_install_and_delivers_after() {
        // 未 install: 送り先がなくても落ちない（no-op）。
        record(UsageEvent::Server {
            feature: "search",
            status: 200,
        });

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        install(tx);
        record(UsageEvent::Llm {
            purpose: "summarize",
            model: "claude-sonnet-4-6".into(),
            input_tokens: 100,
            output_tokens: 20,
        });
        match rx.recv().await {
            Some(UsageEvent::Llm {
                purpose,
                model,
                input_tokens,
                output_tokens,
            }) => {
                assert_eq!(purpose, "summarize");
                assert_eq!(model, "claude-sonnet-4-6");
                assert_eq!(input_tokens, 100);
                assert_eq!(output_tokens, 20);
            }
            other => panic!("expected Llm event, got {other:?}"),
        }

        // 受信側を落としても record は panic しない。
        drop(rx);
        record(UsageEvent::Server {
            feature: "search",
            status: 200,
        });
    }
}
