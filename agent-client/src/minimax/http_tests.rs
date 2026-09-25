use std::collections::VecDeque;
use std::sync::Arc;

use axum::{http::HeaderMap, routing::post, Json, Router};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use super::{MiniMaxConfig, MiniMaxInvoker, MiniMaxProtocol};
use crate::driver::LlmBackend;

struct MockApi {
    base_url: String,
    invoker: MiniMaxInvoker,
    requests: mpsc::UnboundedReceiver<Value>,
    server: tokio::task::JoinHandle<()>,
}

impl MockApi {
    async fn start(protocol: MiniMaxProtocol, responses: Vec<Value>) -> Self {
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let (tx, requests) = mpsc::unbounded_channel();
        let path = match protocol {
            MiniMaxProtocol::Openai => "/chat/completions",
            MiniMaxProtocol::Anthropic => "/v1/messages",
        };
        let app = Router::new().route(
            path,
            post(move |headers: HeaderMap, Json(request): Json<Value>| {
                let tx = tx.clone();
                let responses = Arc::clone(&responses);
                async move {
                    assert_eq!(headers["authorization"], "Bearer test-key");
                    tx.send(request).unwrap();
                    Json(responses.lock().await.pop_front().unwrap())
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let config = MiniMaxConfig {
            api_key: "test-key".into(),
            protocol,
            global_openai_base_url: base_url.clone(),
            global_anthropic_base_url: base_url.clone(),
            ..Default::default()
        };
        Self {
            base_url,
            invoker: MiniMaxInvoker::new(&config, "system".into()).unwrap(),
            requests,
            server,
        }
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        self.server.abort();
    }
}

fn reply(protocol: MiniMaxProtocol, text: &str) -> Value {
    match protocol {
        MiniMaxProtocol::Openai => json!({
            "choices": [{"message": {"content": text}, "finish_reason": "stop"}]
        }),
        MiniMaxProtocol::Anthropic => json!({
            "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"
        }),
    }
}

#[tokio::test]
async fn openai_preserves_reasoning_without_returning_it_as_reply_text() {
    let details = json!([
        {"type": "reasoning.text", "text": "plan", "signature": "sig"},
        {"type": "reasoning.encrypted", "data": "opaque", "index": 1}
    ]);
    let response = json!({"choices": [{"message": {
        "role": "assistant",
        "content": "{\"actions\":[]}",
        "reasoning_content": "plan",
        "reasoning_details": details
    }, "finish_reason": "stop"}]});
    let mut api = MockApi::start(MiniMaxProtocol::Openai, vec![response.clone(), response]).await;
    assert_eq!(
        api.invoker.send_message("first").await.unwrap(),
        "{\"actions\":[]}"
    );
    api.invoker.send_message("second").await.unwrap();
    let first = api.requests.recv().await.unwrap();
    let second = api.requests.recv().await.unwrap();
    assert_eq!(first["reasoning_split"], true);
    assert_eq!(first["max_completion_tokens"], 1024);
    assert!(first.get("max_tokens").is_none());
    assert_eq!(second["messages"][2]["reasoning_content"], "plan");
    assert_eq!(second["messages"][2]["reasoning_details"], details);
    for index in [0, 1, 3] {
        assert!(second["messages"][index].get("reasoning_content").is_none());
        assert!(second["messages"][index].get("reasoning_details").is_none());
    }
}

#[tokio::test]
async fn anthropic_preserves_thinking_without_returning_it_as_reply_text() {
    let blocks = json!([
        {"type": "thinking", "thinking": "plan", "signature": "sig"},
        {"type": "text", "text": "{\"actions\":[]}"}
    ]);
    let response = json!({"content": blocks, "stop_reason": "end_turn"});
    let mut api =
        MockApi::start(MiniMaxProtocol::Anthropic, vec![response.clone(), response]).await;
    assert_eq!(
        api.invoker.send_message("first").await.unwrap(),
        "{\"actions\":[]}"
    );
    api.invoker.send_message("second").await.unwrap();
    api.requests.recv().await.unwrap();
    let second = api.requests.recv().await.unwrap();
    assert_eq!(second["messages"][1]["content"], blocks);
}

#[tokio::test]
async fn failed_completions_preserve_the_previous_conversation() {
    let cases = [
        (
            MiniMaxProtocol::Anthropic,
            json!({
                "content": [{"type": "thinking", "thinking": "unfinished plan"}],
                "stop_reason": "max_tokens"
            }),
            "token limit",
        ),
        (
            MiniMaxProtocol::Anthropic,
            json!({"content": [{"type": "text", "text": "partial"}], "stop_reason": "max_tokens"}),
            "token limit",
        ),
        (
            MiniMaxProtocol::Anthropic,
            json!({"content": [{"type": "thinking", "thinking": "plan"}], "stop_reason": "end_turn"}),
            "no reply text",
        ),
        (
            MiniMaxProtocol::Anthropic,
            reply(MiniMaxProtocol::Anthropic, " \n"),
            "no reply text",
        ),
        (
            MiniMaxProtocol::Openai,
            json!({"choices": [{"message": {"content": null, "reasoning_content": "plan"}, "finish_reason": "length"}]}),
            "token limit",
        ),
        (
            MiniMaxProtocol::Openai,
            json!({"choices": [{"message": {"content": "partial"}, "finish_reason": "length"}]}),
            "token limit",
        ),
        (
            MiniMaxProtocol::Openai,
            json!({"choices": [{"message": {"content": null, "reasoning_content": "plan"}, "finish_reason": "stop"}]}),
            "no reply text",
        ),
        (
            MiniMaxProtocol::Openai,
            reply(MiniMaxProtocol::Openai, " \n"),
            "no reply text",
        ),
        (
            MiniMaxProtocol::Openai,
            json!({"choices": []}),
            "no choices",
        ),
    ];
    for (protocol, failure, expected_error) in cases {
        let mut api = MockApi::start(
            protocol,
            vec![
                reply(protocol, "first reply"),
                failure,
                reply(protocol, "retry reply"),
            ],
        )
        .await;
        api.invoker.send_message("first").await.unwrap();
        let error = api.invoker.send_message("failed").await.unwrap_err();
        assert!(error.to_string().contains(expected_error), "{error}");
        assert_eq!(
            api.invoker.send_message("retry").await.unwrap(),
            "retry reply"
        );
        api.requests.recv().await.unwrap();
        let failed_request = api.requests.recv().await.unwrap();
        let retry_request = api.requests.recv().await.unwrap();
        let mut expected_messages = failed_request["messages"].as_array().unwrap().clone();
        expected_messages.last_mut().unwrap()["content"] = json!("retry");
        assert_eq!(retry_request["messages"], json!(expected_messages));
    }
}

#[tokio::test]
async fn generic_openai_requests_keep_their_existing_wire_format() {
    let protocol = MiniMaxProtocol::Openai;
    let mut api = MockApi::start(
        protocol,
        vec![reply(protocol, "first reply"), reply(protocol, "ok")],
    )
    .await;
    let config = crate::openai::OpenAiConfig {
        base_url: api.base_url.clone(),
        api_key: "test-key".into(),
        model: "some-model".into(),
        ..Default::default()
    };
    let invoker = crate::openai::OpenAiInvoker::new(config.endpoint().unwrap(), "system".into());
    invoker.send_message("first").await.unwrap();
    invoker.send_message("second").await.unwrap();
    api.requests.recv().await.unwrap();
    let request = api.requests.recv().await.unwrap();
    assert_eq!(
        request,
        json!({
            "model": "some-model",
            "messages": [
                {"role": "system", "content": "system"},
                {"role": "user", "content": "first"},
                {"role": "assistant", "content": "first reply"},
                {"role": "user", "content": "second"}
            ],
            "max_tokens": 1024,
            "temperature": 0.7,
            "reasoning_effort": "none"
        })
    );
}
