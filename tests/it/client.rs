//! A typed OpenAI client that this project does not write, `async-openai`,
//! talking to the server. It is the independent judge of whether a reply
//! has the OpenAI shape.

use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::types::chat::{
    ChatCompletionMessageToolCalls, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionStreamOptions, ChatCompletionTool, ChatCompletionTools,
    CreateChatCompletionRequest, CreateChatCompletionRequestArgs, FinishReason, FunctionObjectArgs,
    Role,
};
use futures_util::StreamExt as _;
use serde_json::json;

use crate::support::{Server, client};

/// An `async-openai` client that talks to `server`.
pub(crate) fn openai(server: &Server) -> Client<OpenAIConfig> {
    let config = OpenAIConfig::new()
        .with_api_base(server.base_url())
        .with_api_key("sk-any-key-at-all");
    Client::with_config(config).with_http_client(client())
}

/// A request to `model` with a system message and one user message.
fn request(model: &str, user: &str) -> CreateChatCompletionRequest {
    let system = ChatCompletionRequestSystemMessageArgs::default()
        .content("You are a helpful assistant.")
        .build()
        .expect("a system message");
    let user = ChatCompletionRequestUserMessageArgs::default()
        .content(user)
        .build()
        .expect("a user message");
    CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages([system.into(), user.into()])
        .build()
        .expect("a chat request")
}

#[tokio::test]
async fn a_typed_openai_client_reads_the_reply() {
    let server = Server::start().await;
    let response = openai(&server)
        .chat()
        .create(request("cheapseek", "Hi, I'm Sam."))
        .await
        .expect("the client reads the reply");

    assert_eq!(response.object, "chat.completion");
    assert_eq!(response.model, "cheapseek");
    assert_eq!(response.created, 1_795_305_600);
    assert_eq!(response.choices.len(), 1);
    let choice = &response.choices[0];
    assert_eq!(choice.index, 0);
    assert_eq!(choice.message.role, Role::Assistant);
    assert_eq!(
        choice.message.content.as_deref(),
        Some("Nice to meet you, Sam. What can I help with?")
    );
    assert_eq!(choice.finish_reason, Some(FinishReason::Stop));
    let usage = response.usage.expect("usage");
    assert_eq!(usage.completion_tokens, 13);
    assert_eq!(
        usage.total_tokens,
        usage.prompt_tokens + usage.completion_tokens
    );
}

#[tokio::test]
async fn a_typed_openai_client_reads_the_stream_to_its_end() {
    let server = Server::start().await;
    let mut request = request("cheapseek", "Hi, I'm Sam.");
    request.stream_options = Some(ChatCompletionStreamOptions {
        include_usage: Some(true),
        ..ChatCompletionStreamOptions::default()
    });
    let mut stream = openai(&server)
        .chat()
        .create_stream(request)
        .await
        .expect("the client opens the stream");

    let mut text = String::new();
    let mut finish = None;
    let mut usage = None;
    let mut chunks = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("the client reads the chunk");
        chunks += 1;
        assert_eq!(chunk.object, "chat.completion.chunk");
        for choice in &chunk.choices {
            if let Some(content) = &choice.delta.content {
                text.push_str(content);
            }
            if choice.finish_reason.is_some() {
                finish = choice.finish_reason;
            }
        }
        if chunk.usage.is_some() {
            usage = chunk.usage;
        }
    }

    assert_eq!(text, "Nice to meet you, Sam. What can I help with?");
    assert_eq!(finish, Some(FinishReason::Stop));
    let usage = usage.expect("the last chunk holds the usage");
    assert_eq!(usage.completion_tokens, 13);
    // The role chunk, 13 tokens, the finish chunk, and the usage chunk.
    assert_eq!(chunks, 16);
}

/// The `get_weather` tool, as the typed client describes a tool.
fn weather_tool() -> ChatCompletionTools {
    let function = FunctionObjectArgs::default()
        .name("get_weather")
        .description("Get the weather for a city.")
        .parameters(json!({
            "type": "object",
            "properties": { "city": { "type": "string" } },
            "required": ["city"]
        }))
        .build()
        .expect("a function");
    ChatCompletionTools::Function(ChatCompletionTool { function })
}

/// A request to the chat model with the weather tool and these messages.
fn tool_request(messages: Vec<ChatCompletionRequestMessage>) -> CreateChatCompletionRequest {
    CreateChatCompletionRequestArgs::default()
        .model("cheapseek")
        .messages(messages)
        .tools([weather_tool()])
        .build()
        .expect("a chat request")
}

#[tokio::test]
async fn a_typed_openai_client_drives_a_full_tool_loop() {
    let server = Server::start().await;
    let openai = openai(&server);
    let user: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
        .content("Please get weather for Oslo.")
        .build()
        .expect("a user message")
        .into();

    let first = openai
        .chat()
        .create(tool_request(vec![user.clone()]))
        .await
        .expect("the client reads the tool calls");
    let choice = &first.choices[0];
    assert_eq!(choice.finish_reason, Some(FinishReason::ToolCalls));
    assert_eq!(choice.message.content, None);
    let calls = choice.message.tool_calls.clone().expect("tool calls");
    let [ChatCompletionMessageToolCalls::Function(call)] = calls.as_slice() else {
        panic!("one function call, and this is {calls:?}");
    };
    assert_eq!(call.function.name, "get_weather");
    assert_eq!(call.function.arguments, r#"{"city":"a"}"#);

    // The harness runs the tool and sends the result back.
    let assistant: ChatCompletionRequestMessage =
        ChatCompletionRequestAssistantMessageArgs::default()
            .tool_calls(calls.clone())
            .build()
            .expect("an assistant message")
            .into();
    let result: ChatCompletionRequestMessage = ChatCompletionRequestToolMessageArgs::default()
        .tool_call_id(call.id.clone())
        .content("4 C and rain")
        .build()
        .expect("a tool message")
        .into();
    let second = openai
        .chat()
        .create(tool_request(vec![user, assistant, result]))
        .await
        .expect("the client reads the last reply");
    let choice = &second.choices[0];
    assert_eq!(choice.finish_reason, Some(FinishReason::Stop));
    assert_eq!(
        choice.message.content.as_deref(),
        Some("get_weather: 4 C and rain")
    );
    assert_eq!(choice.message.tool_calls, None);
}

#[tokio::test]
async fn a_typed_openai_client_reads_a_streamed_tool_call() {
    let server = Server::start().await;
    let user: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
        .content(r#"[[call get_weather {"city":"Oslo"}]]"#)
        .build()
        .expect("a user message")
        .into();
    let mut stream = openai(&server)
        .chat()
        .create_stream(tool_request(vec![user]))
        .await
        .expect("the client opens the stream");

    let mut id = String::new();
    let mut name = String::new();
    let mut arguments = String::new();
    let mut finish = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("the client reads the chunk");
        for choice in chunk.choices {
            finish = choice.finish_reason.or(finish);
            for call in choice.delta.tool_calls.into_iter().flatten() {
                assert_eq!(call.index, 0);
                id.push_str(call.id.as_deref().unwrap_or_default());
                let function = call.function.expect("a function in every piece");
                name.push_str(function.name.as_deref().unwrap_or_default());
                arguments.push_str(function.arguments.as_deref().unwrap_or_default());
            }
        }
    }
    assert!(id.starts_with("call_"), "{id}");
    assert_eq!(name, "get_weather");
    assert_eq!(arguments, r#"{"city":"Oslo"}"#);
    assert_eq!(finish, Some(FinishReason::ToolCalls));
}

#[tokio::test]
async fn a_typed_openai_client_reads_the_error_envelope() {
    let server = Server::start().await;
    let error = openai(&server)
        .chat()
        .create(request("gpt-4o", "hello"))
        .await
        .expect_err("the server lacks the model");
    let OpenAIError::ApiError(response) = error else {
        panic!("the client reports an API error, and this is {error:?}");
    };
    assert_eq!(response.status_code.as_u16(), 404);
    let error = response.api_error;
    assert_eq!(error.message, "The model `gpt-4o` does not exist.");
    assert_eq!(error.param.as_deref(), Some("model"));
    assert_eq!(error.code.as_deref(), Some("model_not_found"));
}
