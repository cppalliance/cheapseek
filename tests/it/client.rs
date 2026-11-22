//! A typed OpenAI client that this project does not write, `async-openai`,
//! talking to the server. It is the independent judge of whether a reply
//! has the OpenAI shape.

use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::types::chat::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequest, CreateChatCompletionRequestArgs, FinishReason, Role,
};

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
