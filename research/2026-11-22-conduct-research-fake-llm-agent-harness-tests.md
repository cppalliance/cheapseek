---
produced: 2026-11-22
title: Fake LLM for tests - how agent harnesses and coding agents test their agent loop without a model (Codex CLI wiremock SSE fixtures, Goose TestProvider and OpenAiFixture, Zed FakeLanguageModelProvider, Pi faux provider, opencode simulation, Gemini CLI fake responses, OpenHands TestLLM, SWE-agent replay models, Rust HTTP test doubles)
---

# How agent harnesses test their agent loop without calling a model

## How to read this

- Everything below comes from a page or source file I opened on 2026-11-22. Each section lists the URLs.
- The fetch tool turns pages into markdown and drops some angle-bracket generics from Rust and TypeScript (for example `Vec<Value>` shows up as `Vec`). Code excerpts are trimmed to lines that survived intact. Treat them as illustrations of style and check the source before copying.
- Several repos have moved since their well-known names: `block/goose` now resolves to `aaif-goose/goose`, `sst/opencode` to `anomalyco/opencode`, `badlogic/pi-mono` to `earendil-works/pi`, `All-Hands-AI/OpenHands` to `OpenHands/OpenHands`, and Letta's live code sits in `letta-ai/letta-code`.
- Each section answers the same questions: what replaces the model, how a test writes its script, how tool rounds and failures are simulated, what tests assert on, how determinism is kept, and whether any stand-in produces content by rule.

## OpenAI Codex CLI (Rust, `openai/codex`, `codex-rs`)

- Source files opened:
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/common/responses.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/common/streaming_sse.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/common/context_snapshot.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/suite/tool_harness.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/suite/tool_parallelism.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/suite/stream_no_completed.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/suite/stream_error_allows_next_turn.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/suite/retry_after.rs
  - https://github.com/openai/codex/blob/main/codex-rs/core/tests/suite/scenarios.rs

### What replaces the model

- A local HTTP server. The real client code, the real SSE parser and the real retry logic all run. Only the far end of the socket is fake.
- Three servers live in the test-support crate (imported as `core_test_support`, source directory `codex-rs/core/tests/common`):
  - A `wiremock::MockServer` started by `start_mock_server()`. It also mounts a default `/models` reply so tests stay hermetic.
  - A hand-written TCP server in `streaming_sse.rs`, started by `start_streaming_sse_server(...)`, for tests that need to control when each chunk of a stream is released.
  - A WebSocket server, `start_websocket_server(...)`, for the Responses-over-WebSocket transport.
- Codex speaks the OpenAI Responses API here (`POST .../responses`), so the fixtures are Responses events such as `response.output_item.done`, and chat-completions chunks are a different wire format.

### How a test writes a script

- The names you remembered are real: `mount_sse_once`, `sse(...)`, `ev_function_call`, `ev_assistant_message`, `ev_completed` all exist in `core_test_support::responses`.
- `sse(events)` takes a list of JSON values and renders an SSE body, one `event: <type>` line plus one `data: <json>` line per event.
- Event builders (each returns a `serde_json::Value`): `ev_response_created`, `ev_completed`, `ev_completed_with_tokens`, `ev_assistant_message`, `ev_message_item_added`, `ev_output_text_delta`, `ev_reasoning_item`, `ev_reasoning_item_added`, `ev_reasoning_summary_text_delta`, `ev_reasoning_text_delta`, `ev_function_call`, `ev_function_call_with_namespace`, `ev_custom_tool_call`, `ev_apply_patch_custom_tool_call`, `ev_exec_command_call`, `ev_web_search_call_done`, `ev_image_generation_call`, `ev_tool_search_call`. Whole-body shortcuts: `sse_completed(id)` and `sse_failed(id, code, message)`.
- Mount helpers, all matching `POST` on path regex `.*/responses$`:
  - `mount_sse_once(server, body)` - serves this body to one request (`up_to_n_times(1)`). Call it twice to script two rounds.
  - `mount_sse_once_match(server, matcher, body)` and `mount_response_once_match(...)` - same, plus an extra wiremock matcher on the request.
  - `mount_sse_sequence(server, bodies)` - serves the bodies in order from a counter, panics with `no response for {call_num}` when a request arrives past the end, and sets `.expect(num_calls)` so wiremock checks the exact call count.
  - `mount_response_sequence(server, responses)` - same idea with full `ResponseTemplate` values, so a 429 or 503 can sit in the queue next to an SSE reply.
  - `mount_response_handler(server, closure)` - the closure gets the decoded request and picks the response. This is the one rule-driven entry point.
  - `mount_function_call_agent_response(server, call_id, arguments, tool_name)` - a canned two-round script: a function call, then an assistant message "done".
- A tool-call round followed by an assertion on the follow-up request, from `tool_harness.rs`:

```rust
let first_response = sse(vec![
    ev_response_created("resp-1"),
    ev_function_call(call_id, "exec_command", &command_args),
    ev_completed("resp-1"),
]);
responses::mount_sse_once(&server, first_response).await;

let second_response = sse(vec![
    ev_assistant_message("msg-1", "all done"),
    ev_completed("resp-2"),
]);
let second_mock = responses::mount_sse_once(&server, second_response).await;

// ... start the turn, then:
wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;

let req = second_mock.single_request();
let (output_text, _) = call_output(&req, call_id);
```

- `call_output` in that test file reads `req.function_call_output(call_id)` and `req.function_call_output_content_and_success(call_id)`, then the test matches the tool output text with a regex.

### Tool rounds, parallel calls, streaming, reasoning, errors

- Parallel tool calls: put several `ev_function_call` events in one response. `tool_parallelism.rs` does this with `call-1`, `call-2`, `call-3`, then checks in the follow-up request that all `function_call` items come before all `function_call_output` items and that outputs keep call order.
- Streaming deltas: `ev_output_text_delta`, `ev_reasoning_summary_text_delta`, `ev_reasoning_text_delta`. Reasoning items: `ev_reasoning_item(id, summary, raw_content)` also fills an `encrypted_content` field with base64 padding.
- Controlled chunk timing: `StreamingSseChunk { gate: Option<oneshot::Receiver<()>>, body: String }`. The server writes a chunk only after its gate fires, so a test can hold `response.completed` back while it checks that tools already started. `start_streaming_sse_server` also returns one receiver per response that reports the time the final chunk was sent.

```rust
let (streaming_server, completion_receivers) = start_streaming_sse_server(vec![
    vec![
        StreamingSseChunk { gate: Some(first_gate_rx), body: first_chunk },
        StreamingSseChunk { gate: Some(completion_gate_rx), body: second_chunk },
    ],
    vec![StreamingSseChunk { gate: Some(follow_up_gate_rx), body: follow_up }],
])
.await;
```

- Cut stream: `stream_no_completed.rs` serves a body that lacks `response.completed`, then a complete one, sets `stream_max_retries: Some(1)` on the provider config, and asserts the server saw 2 requests.
- Rate limit: `retry_after.rs` queues `ResponseTemplate::new(429).insert_header("Retry-After", "1")` followed by a good SSE reply through `mount_response_sequence`, then asserts on a retry telemetry event and on `response_mock.requests().len() == 2`.
- Server error: `stream_error_allows_next_turn.rs` mounts a 500 JSON error matched by `body_string_contains("first message")` and a good reply matched by `body_string_contains("follow up")`. Here the request matcher does the routing, so order of arrival does not matter.
- Unreachable provider: one test binds a port, drops the listener, points the client at it, waits for a `StreamError` event, then starts a `MockServer` on that same port.
- WebSocket transport: `start_websocket_server(connections)` takes, per connection, a queue of event batches (one batch per request). `gate_response_event(&event)` pauses the server right before a given event until the test releases it. Config fields `accept_delay` and `close_after_requests` simulate a slow handshake and a peer that never closes.

### What tests assert on

- The requests the harness sent. `ResponseMock` implements wiremock's `Match` trait, so it records every request while matching. Accessors: `single_request()` (panics unless exactly one), `requests()`, `last_request()`, `saw_function_call(call_id)`, `function_call_output_text(call_id)`.
- `ResponsesRequest` wraps a recorded request with helpers: `body_json()`, `input()`, `inputs_of_type(ty)`, `message_input_texts(role)`, `function_call_output(call_id)`, `custom_tool_call_output(call_id)`, `tool_by_name(namespace, name)`, `header(name)`, `body_contains_text(text)`. It decodes zstd-compressed bodies first.
- Events from the agent, through `wait_for_event(&codex, predicate)`, for example `EventMsg::PlanUpdate`, `EventMsg::PatchApplyBegin`, `EventMsg::TurnComplete`.
- Side effects on disk (a patched file's contents).
- Snapshots of the model-visible context. `context_snapshot.rs` renders captured requests as readable text and normalizes volatile parts (paths, IDs). The header comment of `scenarios.rs` says its multi-turn scenarios snapshot the model-visible request history of shipped features.

### How determinism is kept

- Ordered one-shot mounts and counted sequences. A request past the end of a sequence panics, and `.expect(n)` fails the test when fewer arrive.
- Every captured request passes through `validate_request_body_invariants`, which panics when a tool output lacks a matching call or a call lacks an output. This check runs in the mock itself, so every test gets it for free.
- Gates (oneshot channels) decide stream timing, so tests wait on signals and not on sleeps. Some tests still poll with short sleeps and wall-clock bounds (for example `elapsed < Duration::from_millis(1_600)` in a parallelism test).
- Tests that talk to a mock server mostly run on `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` and on real time.
- Retry backoff math is tested apart from the server, on a paused tokio clock. `retry_after.rs` has tests marked `#[tokio::test(start_paused = true)]` that call `run_with_retry` with a closure in place of the transport ("Check backoff without a live server", says the comment), record `tokio::time::Instant::now()` per attempt, and use `tokio::time::advance(...)` to move time.
- The streaming SSE server answers 500 with `no responses queued` once its queue is empty.

### Rule-driven stand-ins

- `mount_response_handler` lets a closure compute the reply from the request. Everything else I opened is scripted.

## Block's Goose (Rust, now `aaif-goose/goose`)

- Source files opened:
  - https://github.com/aaif-goose/goose/blob/main/crates/goose/src/providers/testprovider.rs
  - https://github.com/aaif-goose/goose/blob/main/crates/goose/tests/agent.rs
  - https://github.com/aaif-goose/goose/blob/main/crates/goose/tests/acp_fixtures/mod.rs
  - https://github.com/aaif-goose/goose/blob/main/crates/goose/tests/acp_fixtures/provider.rs
  - https://github.com/aaif-goose/goose/blob/main/crates/goose/tests/acp_common_tests/mod.rs
  - https://github.com/aaif-goose/goose/blob/main/crates/goose/tests/acp_test_data/openai_tool_call.txt

### What replaces the model

Goose uses three different stand-ins, each at a different layer.

- In-process fakes behind the `Provider` trait. `crates/goose/tests/agent.rs` defines a small struct per test group and implements `Provider::stream` by hand: `MockToolProvider`, `MultiStepProvider`, `ThinkingStreamProvider`, `CombinedThinkingToolProvider`, `SummarizationTestProvider`, `FixedUsageProvider`, `EmptyThenTextProvider`, and more.
- Record and replay behind the same trait. `TestProvider` wraps a real provider in recording mode and answers from a JSON file in replay mode.
- A local HTTP server with recorded SSE files. `OpenAiFixture` is a wiremock server on `POST /v1/chat/completions`, used by the ACP tests, and the real `OpenAiProvider` talks to it.

### How a test writes a script

- Hand-written provider with a call counter, from `MultiStepProvider` in `agent.rs`. Call 0 returns a tool request, call 1 streams five text tokens, later calls stream tokens and fire a cancellation token after the second one:

```rust
let call = self.call_count.fetch_add(1, Ordering::SeqCst);
match call {
    0 => {
        let tool_call = CallToolRequestParams::new("test_tool")
            .with_arguments(object!({"param": "value"}));
        let message =
            Message::assistant().with_tool_request("call_1", Ok(tool_call));
        // ... single-item stream
    }
    1 => {
        let tokens = vec!["Hello", " world", ", how", " are", " you?"];
        // ... one stream item per token, usage on the last one
    }
    _ => { /* stream tokens, call cancel.cancel() at i == 1 */ }
}
```

- `TestProvider` (record and replay):
  - `TestProvider::new_recording(inner, file_path)` calls the inner provider, collects the stream, and stores `{ input: { system, messages, tools }, output: { message, usage } }` under a key.
  - The key is a SHA-256 hash of the messages (role plus content), after stripping internal metadata such as `tool_meta`. The system prompt and tool list are stored but stay out of the key.
  - `TestProvider::new_replaying(file_path)` looks the hash up and returns the stored message as a single-item stream. A miss returns `ProviderError::ExecutionError("No recorded response found for input hash: ...")`.
  - `finish_recording()` writes the map as pretty JSON.
- `OpenAiFixture` (HTTP level):
  - The script is a list of `(pattern, response)` pairs kept in a `VecDeque`.
  - Each request body must contain the pattern at the front of the queue. On a match the entry is popped and the response is sent as `text/event-stream`.
  - On a mismatch the server answers HTTP 417 with an OpenAI-shaped error whose message shows the expected pattern and the actual body. An extra request with an empty queue gets 417 `Unexpected request`.
  - A response string of the form `status:<code> <body>` becomes an HTTP error with that status.
  - `with_response_delay(exchanges, expected_session_id, delay)` adds a fixed delay through wiremock's `set_delay`. `reset()` refills the queue.
  - Responses are files of raw chat-completions SSE captured from a real model (`acp_test_data/openai_tool_call.txt` has `chat.completion.chunk` lines with `tool_calls` deltas, a usage chunk, and `data: [DONE]`).

```rust
let openai = OpenAiFixture::new(
    vec![
        (
            prompt.to_string(),
            include_str!("../acp_test_data/openai_tool_call.txt"),
        ),
        (
            format!(r#""content":"{FAKE_CODE}""#),
            include_str!("../acp_test_data/openai_tool_result.txt"),
        ),
    ],
    expected_session_id.clone(),
)
.await;
```

- The second pattern is the tool result text, so the script itself proves that the round-two request contained the tool output.

### Tool rounds, streaming, errors

- Tool round: a provider that returns `Message::assistant().with_tool_request(id, Ok(call))`, or an SSE file with `tool_calls` deltas and `finish_reason: "tool_calls"`.
- Streaming deltas: a `futures::stream::iter` of partial messages sharing one message id.
- Thinking: provider names `ThinkingStreamProvider` and `CombinedThinkingToolProvider` exist in `agent.rs`. I read their names and positions but did not open their bodies.
- Max-turns limit: `MockToolProvider` returns the same tool call forever, and the test sets `max_turns: Some(1)`.
- Cancellation mid-stream: the provider itself fires the `CancellationToken` at a chosen token index.
- HTTP errors: the `status:<code>` form in `OpenAiFixture`.
- The tree also has `crates/goose/tests/mcp_replays/` and `crates/goose-test/src/mcp/stdio/record.rs`, which by name record and replay MCP server traffic. I saw the paths only.

### What tests assert on

- The stream of `AgentEvent` values returned by `services.reply(...)`.
- ACP-level output text and a simplified notification list, for example `[Notification::ToolCall, Notification::ToolCallContent("content"), Notification::ToolCallStatus(Completed)]`.
- Request content, indirectly, through the fixture's body patterns and a session-id header check.

### How determinism is kept

- Strict queue order with a loud 417 on mismatch.
- Replay fails on a missing hash.
- The ACP tests take a global lock (`ACP_TEST_LOCK`) and run one at a time.

### Rule-driven stand-ins

- `MockToolProvider` (always the same tool call) and the counter-based providers are tiny rule bots: the reply depends on the call number only.

### Scenario tests

- The path `crates/goose-cli/src/scenario_tests` returns 404 on the current main branch, and my filter over the current file tree for the word "scenario" returned zero paths. Goose scenario tests stay unconfirmed.

## Zed (Rust, `zed-industries/zed`)

- Source files opened:
  - https://github.com/zed-industries/zed/blob/main/crates/language_model/src/fake_provider.rs
  - https://github.com/zed-industries/zed/blob/main/crates/agent/src/tests/mod.rs

### What replaces the model

- An in-process fake behind the provider trait: `FakeLanguageModelProvider` in `crates/language_model/src/fake_provider.rs`. Its doc comment says it "serves any model id and records every request".
- In the file as it stands today the one fake type is `FakeLanguageModelProvider`, and the models it hands out are plain `LanguageModel` values (default id `fake`). A type named `FakeLanguageModel` stays unconfirmed.
- The test supplies every reply by hand, live, while the agent runs. Each call to `stream_completion` stores a `PendingCompletion { model_id, request, events }` where `events` is the sending half of an unbounded channel, and returns the receiving half as the response stream. The test then pushes events into that channel by hand.

### How a test writes a script

- The test is the script. It interleaves "let the agent run" with "push the next model event":
  - `fake.send_last_text(&model, "Hello")` - push a text chunk to the newest pending request.
  - `fake.send_last_event(&model, LanguageModelCompletionEvent::ToolUse(tool_use))` - push any event, including tool use and thinking.
  - `fake.send_last_error(&model, error)` - push an error into the stream.
  - `fake.end_last(&model)` - close the stream.
  - `send_text`, `send_event`, `send_error`, `end_stream` do the same for a specific request when several are in flight.
- From `test_prompt_caching`:

```rust
thread
    .update(cx, |thread, cx| {
        thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
    })
    .unwrap();
cx.run_until_parked();

fake.send_last_event(
    &model,
    LanguageModelCompletionEvent::ToolUse(tool_use.clone()),
);
fake.end_last(&model);
cx.run_until_parked();

let completion = fake.pending_completions().pop().unwrap();
// assert_eq!(completion.messages[1..], vec![ ... tool result message ... ]);
```

### Tool rounds, streaming, errors, retries

- Tool round: send a `ToolUse` event, end the stream, run until parked, and the agent's follow-up request shows up in `fake.pending_completions()`.
- Streaming deltas: any number of `send_last_text` calls before `end_last`. Thinking: `LanguageModelCompletionEvent::Thinking { .. }`.
- Errors and retries, from `test_send_retry_on_error`:

```rust
fake.send_last_text(&model, "Hey,");
fake.send_last_error(
    &model,
    LanguageModelCompletionError::from_http_status(
        LanguageModelProviderName::new("Anthropic"),
        http_client::StatusCode::SERVICE_UNAVAILABLE,
        "Anthropic's API servers are overloaded right now".to_string(),
        Some(Duration::from_secs(3)),
    ),
);
fake.end_last(&model);

cx.executor()
    .advance_clock(crate::maximum_retry_delay_with_jitter(Duration::from_secs(3)));
cx.run_until_parked();

fake.send_last_text(&model, "there!");
fake.end_last(&model);
```

- A partial text chunk followed by an error is how a cut stream is expressed.
- `fake.forbid_requests()` makes every new request fail with "requests are forbidden", which catches unexpected model calls.
- `fake.queue_input_token_count(n)` scripts token-count replies, used for compaction thresholds.

### What tests assert on

- The exact request the agent sent: `fake.pending_completions()` returns the recorded `LanguageModelRequest` values, and tests compare `completion.messages` against a literal list, including cache flags.
- The event stream from `thread.send(...)` (for example `ThreadEvent::Retry`, `ThreadEvent::Stop`).
- The final transcript rendered by `thread.to_markdown()`.

### How determinism is kept

- Tests are `#[gpui::test]` functions that receive a `TestAppContext`. `cx.run_until_parked()` runs all ready work to a standstill, and `cx.executor().advance_clock(duration)` moves the fake clock, so a 5 second retry delay costs no wall time.
- Nothing arrives from the model unless the test sends it, so every interleaving is chosen by the test.
- The helper panics with `no pending completion to model ...` when the test tries to answer a request the agent never made.
- Tests against live models share the file and are switched off by default with `#[cfg_attr(not(feature = "e2e"), ignore)]` and `setup(cx, TestModel::Sonnet4)`.

### Rule-driven stand-ins

- The fake model is purely hand-driven. Tools in the tests are simple rule tools (`EchoTool`, `DelayTool`).

## Pi coding agent (TypeScript, now `earendil-works/pi`)

- Source files opened:
  - https://github.com/earendil-works/pi/blob/main/packages/ai/src/providers/faux.ts
  - https://github.com/earendil-works/pi/blob/main/packages/coding-agent/test/suite/harness.ts

### What replaces the model

- An in-process "faux" provider registered next to the real providers. It is the richest in-process fake I found, because it also simulates streaming, usage and prompt caching.

### How a test writes a script

- A queue of steps. Each step is either a ready-made assistant message or a factory function that receives the request context and returns a message.
- Builders: `fauxText(text)`, `fauxThinking(thinking)`, `fauxToolCall(name, arguments, { id })`, and `fauxAssistantMessage(content, { stopReason, errorMessage, responseId, timestamp })`.
- Queue controls: `setResponses(steps)`, `appendResponses(steps)`, `getPendingResponseCount()`, plus `state.callCount`.
- From the doc comment in `faux.ts`:

```ts
const faux = fauxProvider();
const models = createModels();
models.setProvider(faux.provider);
faux.setResponses([fauxAssistantMessage("hi")]);
```

- The factory form is the rule-driven hook. Its signature is `(context, options, state, model) => AssistantMessage | Promise`, so a step can look at the transcript so far and decide what to say.

### Streaming, reasoning, errors, cancellation

- The provider turns each scripted message into a full event stream: `start`, then per block `thinking_start` / `thinking_delta` / `thinking_end`, `text_start` / `text_delta` / `text_end`, `toolcall_start` / `toolcall_delta` / `toolcall_end`, then `done`.
- Text is split into chunks of 3 to 5 "tokens" (4 characters per token) by default, configurable through `tokenSize: { min, max }`. The split uses `Math.random`, so chunk boundaries vary between runs unless min equals max.
- Tool-call arguments are streamed as JSON fragments and parsed by the same partial-JSON parser the real providers use, so consumers see arguments grow.
- Timing: `tokensPerSecond` sets a delay per chunk. With it unset, each chunk is released on a microtask, so tests run at full speed.
- Errors: a message with `stopReason: "error"` or `"aborted"` ends the stream with an `error` event. A factory that throws becomes an error message.
- Cancellation: the abort signal is checked before every chunk and produces an `aborted` message with the partial content.
- Usage: input and output tokens are estimated at length divided by 4. With a session id, it simulates a prompt cache by comparing the new prompt with the previous one and reporting `cacheRead` and `cacheWrite`.
- Deferred (batch-like) responses: options `deferred.pendingFetches` and `deferred.pollAfterMs` simulate a handle that is polled a few times before the scripted answer is ready.

### What tests assert on

- The harness (`createHarness`) collects every `AgentSessionEvent` into `events` and offers `eventsOfType(type)`.
- Transcript helpers: `getUserTexts`, `getAssistantTexts`, `getToolResult(harness, toolName)`.
- `getPendingResponseCount()` lets a test check that the whole script was consumed.

### How determinism is kept

- Ordered queue. When the queue is empty the provider returns an error message with the text `No more faux responses queued`.
- In-memory session manager, settings and auth storage, plus a temp directory per harness.
- Time is real in the files I opened. Speed comes from releasing each chunk on a microtask.

### Rule-driven stand-ins

- Factory steps (see above). The repo tree also lists `apps/p2/plugins/models/fake.ts`, which I did not open.

## opencode (TypeScript and Rust, now `anomalyco/opencode`, branch `dev`)

- Source files opened:
  - https://github.com/anomalyco/opencode/blob/dev/packages/simulation/src/backend/simulated-provider.ts
  - https://github.com/anomalyco/opencode/blob/dev/packages/simulation/src/backend/openai.ts
  - https://github.com/anomalyco/opencode/blob/dev/packages/http-recorder/README.md
  - https://github.com/anomalyco/opencode/blob/dev/packages/cli-rs/docs/testing.md

### What replaces the model

- A simulated provider inside a simulated network (`packages/simulation`). The real OpenAI chat route is claimed by a fake endpoint, so, in the words of the source comment, everything downstream of the response bytes is the real pipeline: SSE framing, the event schema, the protocol state machine.
- Recorded traffic for provider adapters (`packages/http-recorder`, with cassettes under `packages/ai/test/fixtures/recordings/`).

### How a test writes a script

- Driver style, close to Zed's but over a control socket. Each model call becomes a pending invocation with an id (`inv_1`, `inv_2`, ...). A driver then calls `push(id, items)`, `finish(id, reason)` or `disconnect(id)`.
- Items are protocol-level: `textDelta`, `reasoningDelta`, `toolInputStart`, `toolInputDelta`, `toolCall`, and `raw` (any chunk verbatim). Finish reasons: `stop`, `tool-calls`, `length`, `content-filter`.
- `openai.ts` turns those items into chat-completions SSE frames (`choices[].delta.content`, `delta.reasoning_content`, `delta.tool_calls[]`) and ends with `data: [DONE]`.
- Scenario files for the TUI parity suite, per `packages/cli-rs/docs/testing.md`: a scenario is `defineSim({...})` with `model` ("the replies, one per model call, each `{ items, finish }`"), `steps` (keystrokes, `until: "text"`, checkpoints), `files` and `permissions`. A backend driver answers each model call with the scenario's next reply.

### Errors and cut streams

- `disconnect(id)` fails the response stream with `ProviderDisconnectedError` ("Simulated model provider disconnected"), which is a mid-stream cut.
- The `raw` item lets a scenario send a malformed or unusual chunk.

### What tests assert on

- Screen captures at checkpoints, compared between the TypeScript TUI and the Rust client.
- `report.json` with every model call, per the testing doc.

### How determinism is kept

- The doc says: "Wait with `until`, not with time". A checkpoint waits for three identical captures before comparing.
- Each side runs in its own sandbox with an in-memory database, an empty environment and a fresh home.
- Output is normalized (server URL, sandbox root, ids, ISO times, versions, durations, ports).
- The plain fake server for CLI parity answers unrouted requests with 404 and logs them as `(unmatched)`.

### Record and replay (`@opencode/http-recorder`)

- First local run calls the real API and writes a JSON cassette. Later runs replay. With `CI=true`, a missing cassette fails the test.
- A request claims the first unused recorded interaction that matches. Repeated identical requests are served in cassette order, which models retries and polling. Interactions left unused fail when the recorder layer closes.
- Refresh is done by deleting the cassette file. The README says there is intentionally no overwrite mode.
- Secrets are redacted by default, and the recorder scans the cassette for credential-like values before writing.
- Stated limit: responses are buffered during record and replay, so the README says it is "not suitable for tests that assert streaming timing, cancellation, or backpressure".
- Cassette names in the tree show what is covered, for example `bedrock-converse/drives-a-tool-loop.json` and `bedrock-converse/continues-after-parallel-tool-results.json`.

### Rule-driven stand-ins

- The parity backend driver answers "a request with no tools" with a fixed title, per the testing doc. That is a one-line rule next to the script, so that the title-generation call does not eat a scripted reply.

## Gemini CLI (TypeScript, `google-gemini/gemini-cli`)

- Source files opened:
  - https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/core/fakeContentGenerator.ts
  - https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/core/recordingContentGenerator.ts
  - https://github.com/google-gemini/gemini-cli/blob/main/integration-tests/parallel-tools.test.ts
  - https://github.com/google-gemini/gemini-cli/blob/main/integration-tests/parallel-tools.responses
  - https://github.com/google-gemini/gemini-cli/blob/main/docs/integration-tests.md

### What replaces the model

- An in-process fake behind the `ContentGenerator` interface, `FakeContentGenerator`, that ships inside the product and is switched on by a CLI argument named `fake-responses` (with the usual two leading hyphens). The end-to-end tests run the real built binary with that argument.
- Its partner `RecordingContentGenerator` wraps the real generator and appends each response to a file in the same format.

### How a test writes a script

- A file on disk with one JSON object per line. Each line names the API method and holds the response: `generateContent`, `generateContentStream` (a list of chunks), `countTokens`, `embedContent`.
- The whole of `parallel-tools.responses` is three lines. The middle one is a single stream chunk with five `functionCall` parts and a text part:

```json
{"method":"generateContentStream","response":[{"candidates":[{"content":{"parts":[{"functionCall":{"name":"read_file","args":{"file_path":"file1.txt"}}},{"functionCall":{"name":"read_file","args":{"file_path":"file2.txt"}}},{"functionCall":{"name":"write_file","args":{"file_path":"output.txt","content":"wave2"}}},{"functionCall":{"name":"read_file","args":{"file_path":"file3.txt"}}},{"functionCall":{"name":"read_file","args":{"file_path":"file4.txt"}}}, {"text":"All waves completed successfully."}]},"finishReason":"STOP","index":0}]}]}
```

- The test points at the file: `rig.setup('parallel-wave-execution', { fakeResponsesPath: join(import.meta.dirname, 'parallel-tools.responses'), fakeResponsesNonStrict: true, settings: {...} })`.
- Regeneration is built in. The docs call these files goldens and say to run `REGENERATE_MODEL_GOLDENS="true" npm run test:e2e`, with a warning to review the new files for personal data.

### What tests assert on

- Text on the pseudo-terminal (`run.expectText('Allow', 10000)`).
- Tool logs (`rig.readToolLogs()`, `rig.expectToolCallSuccess(['write_file'])`).
- Files on disk.
- The fake ignores request content. It only checks the method name.

### How determinism is kept

- Strict mode (the default) serves responses in file order and throws `No more mock responses for <method>` on overrun and `Unexpected response type` when the next entry is for another method.
- `nonStrict` mode picks the first unused response for the requested method, which the source comment says is "useful for non-deterministic background tasks" (side calls such as routing or summarizing that may arrive in any order).
- The docs ask for a deflake run of at least 5 passes before a new integration test is added.

### Rule-driven stand-ins

- Every reply in the files I opened comes from the script. Stream chunks are yielded back to back, at full speed.

## Crush (Go, `charmbracelet/crush`)

- Source files opened:
  - https://github.com/charmbracelet/crush/blob/main/internal/agent/common_test.go
  - https://github.com/charmbracelet/crush/blob/main/internal/agent/agent_test.go

- What replaces the model: recorded HTTP traffic. `setupAgent` creates `vcr.NewRecorder(t)` from `charm.land/x/vcr` and passes it as the transport of the provider's HTTP client (`openaicompat.WithHTTPClient(&http.Client{Transport: r})`).
- How a test writes a script: the recording is the script. The test sends a real prompt ("Hello", "Read the go mod") and the cassette for that test supplies the model's side.
- What tests assert on: the messages stored for the session, for example that an assistant message holds a call to the view tool (`tools.ViewToolName`) and that a later tool message for the same call id contains `module example.com/testproject`.
- How determinism is kept: everything that feeds the request is pinned so the cassette matches. The system prompt is built with a fixed time function (1/1/2025), `prompt.WithPlatform("linux")` and a fixed working directory under `/tmp/crush-test/<test name>`. Config fields that vary per machine (skills paths, context paths, LSP, attribution) are cleared or fixed, with a code comment saying this is "to ensure cassettes match". Permissions are set to auto-approve so a headless run never blocks.
- The same file has pure unit tests of prompt preparation (`TestPreparePrompt_OrphanedToolUse` and friends) that build message lists by hand and need no model.
- I did not open a cassette file or the `charm.land/x/vcr` source, so the matching rules and the handling of streamed bodies stay unconfirmed.

## Cline (TypeScript, `cline/cline`)

- Source files opened:
  - https://github.com/cline/cline/blob/main/apps/vscode/src/test/e2e/fixtures/server/index.ts
  - https://github.com/cline/cline/blob/main/apps/vscode/src/test/e2e/fixtures/server/api.ts
  - https://github.com/cline/cline/blob/main/sdk/packages/shared/src/vcr.ts
  - https://github.com/cline/cline/blob/main/sdk/packages/llms/src/tests/provider-vcr/README.md

### Rule-driven mock server for end-to-end tests

- What replaces the model: a local HTTP server, `ClineApiServerMock`, that implements `POST /api/v1/chat/completions` plus account endpoints.
- How the reply is chosen: by rules on the request body, not by a queue. This is the clearest rule-driven fake in the survey.
  - Body contains `edit_request` and no tool message yet: stream a lead-in sentence, then a structured `editor` tool call as OpenAI `tool_calls` deltas, then `finish_reason: "tool_calls"`.
  - Body contains `edit_request` and a `role: "tool"` message: stream the closing text. A source comment explains that order matters because the original prompt is still in the history of the follow-up request.
  - Body contains `hook context probe`: answer "received" or "missing" depending on whether a marker (`HOOK_INJECTED_FACT`) appears after the latest probe prompt. The fake thereby reports back whether the harness injected context into the request, and the test asserts on the reply text.
  - Anything else: a default sentence.
- Streaming: the text is split on spaces and written one word per SSE chunk with `setTimeout(sendChunk, chunkDelayMs)`. The delay is 10 ms, or 750 ms for one "probe" turn that a test needs to keep in flight while it changes settings.
- Errors: test-control endpoints under `/.test/` flip server state, for example `setSpendLimitExceeded` makes the chat endpoint answer 429 with a `SPEND_LIMIT_EXCEEDED` body, and a zero balance yields 402.
- One thing to copy with care: the server appends `Generated UUID: <random uuid>` to each reply, so the reply text itself varies between runs.

### VCR for provider adapters and CLI runs

- `sdk/packages/shared/src/vcr.ts` patches `globalThis.fetch` to record and replay HTTP. Its header comment lists the controls:
  - `CLINE_VCR` - `record` or `playback`.
  - `CLINE_VCR_CASSETTE` - cassette path.
  - `CLINE_VCR_FILTER` - only intercept requests whose path contains this substring, so auth calls can go to the real network while inference is replayed.
  - `CLINE_VCR_INCLUDE_REQUEST_BODY` - save sanitized request bodies and assert them during playback.
  - `CLINE_VCR_SSE_DELAY` - milliseconds between SSE chunks during playback, default 100, 0 for instant.
- Redaction is key-based (any JSON key matching a rule gets its value redacted).
- The provider VCR README says cassettes exercise the real provider adapters with no credentials in normal runs, are refreshed with `LLMS_PROVIDER_VCR_RECORD=1`, and that after recording the test normalizes dynamic fields such as response ids, encrypted reasoning payloads and prompt cache keys.

## Continue (TypeScript, `continuedev/continue`)

- Source files opened:
  - https://github.com/continuedev/continue/blob/main/core/llm/llms/Mock.ts
  - https://github.com/continuedev/continue/blob/main/core/llm/llms/Test.ts

- What replaces the model: two built-in providers that are selected by provider name in config, `mock` (`MockLLM`) and `test` (`TestLLM`).
- `MockLLM` script: `chatStreams`, a list of lists of messages, read from `options.requestOptions.extraBodyProperties.chatStream`. The turn is chosen by a rule on the request: index = number of `user` and `tool` messages minus 1. So the script is addressed by conversation position, and a retried request gets the same reply.
- `MockLLM` control tokens inside a script: `"REPEAT_LAST_MSG"` (echo the last message), `"REPEAT_SYSTEM_MSG"` (echo the system message, a neat way to assert on the system prompt the harness built), and `"ERROR"` (throw `Intentional error`).
- With `chatStreams` unset, `MockLLM` streams the string `Test Completion` one character at a time.
- `TestLLM` is a rule bot: a prompt containing `TEST_USER_MESSAGE_<n>` gets `TEST_LLM_RESPONSE_<n>`, any other completion prompt gets `PROMPT: <prompt>` (an echo), and chat always streams `THIS IS A HARDCODED RESPONSE` one character at a time.
- I did not open a test that uses them, so how tool-call rounds are scripted in practice stays unconfirmed. The `ChatMessage` items in `chatStreams` can be tool-call messages by type.

## Aider (Python, `Aider-AI/aider`)

- Source file opened: https://github.com/Aider-AI/aider/blob/main/tests/basic/test_coder.py

- What replaces the model: a patched method. Tests replace `coder.send` with a local function that sets `coder.partial_response_content` to the text the model would have produced (for example a SEARCH/REPLACE edit block) and returns an empty list.

```python
def mock_send(*args, **kwargs):
    coder.partial_response_content = f"""
Do this:

{str(fname)}
<<<<<<< SEARCH
=======
new
>>>>>>> REPLACE

"""
    coder.partial_response_function_call = dict()
    return []

coder.send = mock_send
coder.repo.get_commit_message = MagicMock()
coder.repo.get_commit_message.return_value = "commit message"

coder.run(with_message="hi")
```

- The rest is `unittest.mock`: `MagicMock` for IO and repo calls, `side_effect=[False, True, True]` to script a sequence of user confirmations.
- What tests assert on: file contents after the edit and the number of git commits.
- This is monkeypatching at one call site. It scripts one reply per test and covers Aider's text-edit protocol, which is not tool calling.

## OpenHands (Python, `OpenHands/software-agent-sdk` and `OpenHands/OpenHands`)

- Source files opened:
  - https://github.com/OpenHands/software-agent-sdk/blob/main/openhands-sdk/openhands/sdk/testing/test_llm.py
  - https://github.com/OpenHands/OpenHands/blob/main/tests/e2e/mock-llm/scripts/mock-llm-server.py

### `TestLLM` in the SDK

- What replaces the model: an in-process fake that is a real subclass of `LLM`, shipped in the SDK as `openhands.sdk.testing.TestLLM`. The docstring says it removes the need for `@patch` decorators and knowledge of LiteLLM internals.
- Script: a deque of `Message` or `Exception` objects. A message is returned, an exception is raised, "like unittest.mock side_effect".

```python
llm = TestLLM.from_messages([
    Message(
        role="assistant",
        content=[TextContent(text="")],
        tool_calls=[
            MessageToolCall(
                id="call_1",
                name="my_tool",
                arguments='{"arg": "value"}',
                origin="completion",
            )
        ],
    ),
    Message(role="assistant", content=[TextContent(text="Done!")]),
])
```

- Errors are scripted as exception objects, for example `LLMContextWindowExceedError()`, which is how condensation and fallback paths get tested.
- Determinism: `TestLLMExhaustedError` when the script runs out, plus `remaining_responses` and `call_count` for end-of-test checks. Metrics are zero-cost.
- Limits visible in the source: input messages and tools are ignored, and the `on_token` streaming callback is accepted but unused, so streaming deltas are not simulated.

### Mock LLM server for end-to-end tests

- `mock-llm-server.py` is an OpenAI-compatible HTTP server "powered by openhands-sdk TestLLM". The agent server's LiteLLM layer talks to it.
- Admin API on the same port:
  - `POST /admin/reset` - back to the default trajectory, clear history.
  - `POST /admin/trajectory/register` with `{name, turns}` where each turn is `{"tool_call": {"name", "arguments"}}` or `{"text": "..."}`.
  - `POST /admin/trajectory/activate` with `{name}`.
  - `GET /admin/requests` - every completion request body since the last reset, so a browser test can assert on what the harness sent (the comment mentions checking that image content was included).
- Errors: scripted SDK exceptions map to HTTP statuses through `ERROR_MAP` (401 `invalid_api_key`, 429 `rate_limit_exceeded`, 400 `context_length_exceeded`, 408 `timeout`, 503 `server_error`). An exhausted script answers 500 `Mock LLM exhausted after N calls`.
- Streaming: when the request has `stream: true`, the full reply is re-cut into chunks (role, tool-call header, arguments, finish) and sent with `[DONE]`. A usage chunk is added when `stream_options.include_usage` is set.
- Two rules sit beside the script, both to protect it:
  - A preflight "ping" request (`max_tokens == 1`, user text `ping`) is answered with `pong` outside the script and kept out of the request history. The comment explains that it would otherwise consume a scripted turn.
  - An empty scripted message gets the content `Mock auxiliary response` so the SDK's no-response retry does not eat the next turn.

## SWE-agent and mini-swe-agent (Python)

- Source files opened:
  - https://github.com/SWE-agent/SWE-agent/blob/main/sweagent/agent/models.py
  - https://github.com/SWE-agent/mini-swe-agent/blob/main/src/minisweagent/models/test_models.py

### SWE-agent

- All fakes are in-process model classes selected by model name in config.
- `PredeterminedTestModel(outputs)` - a list of strings or dicts served in order. Dict outputs may include `tool_calls`. Special strings raise errors through `_handle_raise_commands`: `raise_runtime`, `raise_cost` (`CostLimitExceededError`), `raise_context` (`ContextWindowExceededError`), `raise_function_calling <code> [message]` (`FunctionCallingFormatError`).
- `ReplayModel` - reads a replay file (path in `replay_path`) and re-issues the recorded actions one by one. When the recording ends before a submit, it logs an error and submits on its own.
- `InstantEmptySubmitTestModel` - a two-state rule bot: first reply runs `touch reproduce.py`, second reply runs `submit`, then it loops. It sleeps a random time up to `config.delay` first.
- `HumanModel` and `HumanThoughtModel` - a person types the action at a prompt. The same `raise_*` commands work there.

### mini-swe-agent

- `DeterministicModel(outputs=[...])` serves exact output messages in order. `make_output(content, actions, cost)` builds one.
- Variants for each wire style: `DeterministicToolcallModel` with `make_toolcall_output(content, tool_calls, actions)` and `DeterministicResponseAPIToolcallModel` with `make_response_api_output(content, actions)`.
- In-script test actions handled by `_process_test_actions`: an action with a `raise` key raises that exception, a command `/sleep <seconds>` sleeps and then moves to the next output, `/warning <text>` logs a warning and moves on.
- Each call adds `cost_per_call` to global stats, so cost limits can be tested.
- Running past the end of `outputs` raises a plain `IndexError`.

## smolagents (Python, `huggingface/smolagents`)

- Source file opened: https://github.com/huggingface/smolagents/blob/main/tests/test_agents.py

- What replaces the model: small `Model` subclasses defined in the test file, each overriding `generate`.
- They are rule bots keyed on the state of the conversation:
  - `FakeToolCallModel` - when `len(messages) < 3` it returns a tool call to `python_interpreter`, otherwise a `final_answer` tool call.
  - `FakeCodeModel` - when the string `special_marker` is absent from `str(messages)` it returns a step-one reply whose own text contains the marker (so the next call sees it in the transcript), otherwise the final answer.
  - `FakeCodeModelPlanning` checks `planning_marker` then `action_marker`. `FakeCodeModelError` and `FakeCodeModelSyntaxError` return code that fails, to drive the error-recovery path.
- Because the reply depends on what is in the transcript, a retried or repeated call gets a consistent answer with no counter to keep in step.

## Claude Agent SDK and Claude Code

- Source file opened: https://github.com/anthropics/claude-agent-sdk-python/blob/main/tests/test_integration.py

- The Python SDK wraps the Claude Code CLI as a subprocess, so its unit tests fake the CLI and not the model. They patch `SubprocessCLITransport` with an `AsyncMock` and set `mock_transport.read_messages` to an async generator that yields CLI JSON messages (an `assistant` message with `text` and `tool_use` blocks, then a `result` message with `subtype`, `num_turns`, `total_cost_usd`).
- Assertions are on the typed messages the SDK yields and on the options passed to the transport.
- The repo tree also has an `e2e-tests/` directory and a `src/claude_agent_sdk/testing/session_store_conformance.py` module. I did not open them.
- Claude Code's own agent loop is closed source. I found no public test code for it and fetched nothing about it.

## Amazon Q Developer CLI and Kiro CLI (Rust)

- Source files opened:
  - https://github.com/aws/amazon-q-developer-cli/blob/main/crates/chat-cli/src/api_client/mod.rs
  - https://github.com/aws/amazon-q-developer-cli/blob/main/crates/chat-cli/src/cli/chat/mod.rs
  - https://github.com/aws/amazon-q-developer-cli/blob/main/README.md

- What replaces the model: an in-process switch inside the real API client. `ApiClient` has a `mock_client` field holding an iterator of canned event lists, and `send_message` returns `SendMessageOutput::Mock(events)` when it is set.
- Script format: a JSON array of responses. Each response is an array whose items are either a string (assistant text chunk) or an object (a tool use, split into stream events by `split_tool_use_event`).
- The script can also come from a JSON file whose path is read from the environment by `get_mock_chat_response(env)` when the client is built, which lets the built binary run against a script. A search-result excerpt of `util/consts.rs` shows the variable name as `Q_MOCK_CHAT_RESPONSE`; I did not open that file myself.
- From `test_flow` in `cli/chat/mod.rs`:

```rust
os.client.set_mock_output(serde_json::json!([
    [
        "Sure, I'll create a file for you",
        {
            "tool_use_id": "1",
            "name": "fs_write",
            "args": {
                "command": "create",
                "file_text": "Hello, world!",
                "path": "/file.txt",
            }
        }
    ],
    [
        "Hope that looks good to you!",
    ],
]));
```

- The user is scripted too: `InputSource::new_mock(vec!["create a new file", "y", "exit"])` feeds the prompt, the tool approval and the exit command.
- What tests assert on: the fake filesystem (`os.fs.read_to_string("/file.txt")`).
- Overrun behavior: `client.lock().next().unwrap_or_default()` returns an empty event list when the script is used up, so an extra model call passes silently.
- Kiro CLI: the repo README says the open source project now gets only critical security fixes and that the product continues as Kiro CLI, "a closed-source product". Kiro's tests are therefore out of reach.

## Letta (TypeScript, `letta-ai/letta-code`)

- Source files opened:
  - https://github.com/letta-ai/letta/blob/main/README.md
  - https://github.com/letta-ai/letta-code/blob/main/src/backend/dev/fake-headless-backend.ts
  - https://github.com/letta-ai/letta-code/blob/main/src/backend/dev/headless-turn-executor.ts

- The `letta-ai/letta` main branch is now a landing page. Its README says the current source, including the agent harness, lives in `letta-ai/letta-code`, and that the old V1 server is on an `archive` branch. I looked at the current repo only.
- What replaces the model: a fake backend (`fake-headless-backend.ts`, model handle `dev/fake-headless`) that takes a `HeadlessTurnExecutor`. The executor gets the turn input (conversation id, agent, system prompt, body, history) and returns a stream of Letta protocol chunks.
- The executors are all rule bots:
  - `DeterministicPongExecutor` - always an assistant message with the text `pong`, then `stop_reason: end_turn`. This is the default.
  - `DeterministicToolCallExecutor` - when the incoming turn contains approval results, it replies with text `tool result received (<status>): <first 240 chars of the tool return>`. Otherwise it emits an `approval_request_message` for a `Bash` tool call running `echo deterministic-tool-ok`, with ids `tool-call-fake-shell-<n>`, then `stop_reason: requires_approval`.
  - `DeterministicReflectionExecutor` - same shape, but it only calls the tool when the request body contains `$TRANSCRIPT_PATH`. The comment says this makes "a reflection prove it opened the transcript before returning success".
- The tool-call executor's reply quotes the tool result back, so a test can assert on the final text to prove the round trip.
- Timestamps in the fake backend come from a counter (`timestampForRun(sequence)` builds dates from 2026-01-01 plus the sequence number).

## Adjacent libraries with a fake model built in

These are libraries, not coding agents, but two of them show ideas the agents above lack.

- Pydantic AI - https://ai.pydantic.dev/testing/
  - `TestModel` generates content by rule. Per the docs it will "call all tools in the agent, then return either plain text or a structured response", building tool arguments from each tool's JSON schema. The docs say: "There's no ML or AI in `TestModel`, it's just plain old procedural Python code that tries to generate data that satisfies the JSON schema of a tool."
  - `FunctionModel` takes a function `(messages, info) -> ModelResponse`. The doc example branches on `len(messages) == 1` and pulls a date out of the prompt with a regex.
  - `models.ALLOW_MODEL_REQUESTS = False` blocks real model calls globally during tests.
  - `capture_run_messages()` returns the full exchange for assertions, and the doc compares it against a literal list using `IsNow` and `IsStr` matchers for volatile fields.
- Vercel AI SDK - https://ai-sdk.dev/docs/ai-sdk-core/testing
  - `MockLanguageModelV4` takes `doGenerate` as a function or as an array of results served in sequence, and records calls in `doGenerateCalls`.
  - `simulateReadableStream({ chunks, initialDelayInMs, chunkDelayInMs })` builds a stream with a set delay before the first chunk and between chunks.
  - `mockValues` iterates over an array and keeps returning the last value once the array is used up.

## Write-ups on deterministic agent-loop testing

- "Deterministic testing for non-deterministic agent loops", n4n Team, 2026-07-19 - https://n4n.ai/blog/deterministic-testing-for-non-deterministic-agent-loops/
  - Four layers: a `ScriptedModel` behind a narrow model interface that pops queued responses and records calls; record and replay of real traffic as cassettes; contract tests on tool schemas; property-based tests.
  - The property-based idea: generate random scripts (random mix of tool-call replies and final replies) and assert invariants - the loop never crashes, always ends within the step limit, always returns a string.
  - It names what scripted tests miss: malformed JSON from a real model, a different `finish_reason`, argument drift. Its answer is a nightly live suite with loose assertions.
- "Deterministic Replay for Agent Tests", Michael Samuel Naeem, 2026-06-23 - https://blog.michaelsam94.com/agent-deterministic-replay-testing/
  - A `RecordingLLMClient` and `ReplayLLMClient` pair, one fixture file per scenario name, a record flag on the test runner, fixtures committed in the same change as the prompt edit.
  - Store per turn `{messages_hash, tools_offered, response}` and assert in replay that turn n sends the recorded hash, so a prompt regression fails at the first diverging turn.
  - Partial replay: start at turn 3 from a fixture prefix. Keep tool results in fixtures apart from model responses so a tool mock can change without re-recording.
  - Record one fixture per failure mode (tool timeout, invalid args, budget exceeded) because recovery paths are the high-value tests.
  - Tag fixtures with an agent config hash so stale ones are flagged.
- Both posts make the same split: scripted or replayed model in CI for control flow, live model runs nightly for quality.

## Rust HTTP test-double crates

Versions are the ones docs.rs showed on 2026-11-22.

- `wiremock` 0.6.5 - https://docs.rs/wiremock/latest/wiremock/struct.ResponseTemplate.html
  - `ResponseTemplate` sets a whole body (`set_body_string`, `set_body_json`, `set_body_bytes`, `set_body_raw(body, mime)`) and an optional `set_delay(duration)` before the response. The body goes out in one piece.
  - Fit for the job: good for a per-request queue of complete SSE bodies, request matchers, `up_to_n_times`, `.expect(n)` and recorded requests. Codex and Goose both build on it. A custom `Respond` impl or a closure gives sequence or rule behavior.
  - For chunk-by-chunk timing, Codex wrote its own TCP server (`streaming_sse.rs`) and left wiremock for everything else.
- `mockito` 1.7.2 - https://docs.rs/mockito/latest/mockito/struct.Mock.html
  - `with_chunked_body(callback)` hands the callback a writer and uses chunked transfer encoding. The docs say: "You can sleep in between calls to the writer to simulate delays between the chunks. The callback function can also return an error after any number of writes in order to abort the response."
  - That is streaming with controlled chunk timing and a mid-stream cut, in a published crate. The callback is synchronous, so timing is by blocking sleep or by blocking on a channel.
  - Also `with_body_from_request(callback)` for rule-driven bodies, `match_body`, `match_request(closure)`, and `expect(n)` with `assert()`.
- `httpmock` 0.9.0 - https://docs.rs/httpmock/latest/httpmock/struct.Then.html and https://docs.rs/httpmock/latest/httpmock/
  - `Then` offers `status`, `header`, `body`, `body_from_file`, `json_body`, `delay(duration)` for the whole response, `up_to_n_times(n)` (the docs show it for "fails twice, then succeeds" retry tests) and `respond_with(closure)` for dynamic, stateful replies.
  - The crate lists record and playback, forward and proxy modes, a standalone server with a Docker image, and YAML mock files.
  - The `Then` methods I read all produce a complete body, sent after the optional delay.
- `httptest` 0.16.4 - https://docs.rs/httptest/latest/httptest/ and https://docs.rs/httptest/latest/httptest/responders/index.html
  - Expectations with matchers, `times(...)` ranges, and responders: `status_code`, `json_encoded`, `cycle![...]` (rotate through responses) and `delay_and_then`. A function can be a responder and may block.
  - Strictness is the default: a request that matches no expectation gets a 500, and on drop the server panics when an expectation's count was missed or an unexpected request arrived.
  - The responders I read return complete bodies.
- `rvcr` 0.2.1 and `reqwest-vcr` 0.4.0 - https://docs.rs/rvcr/latest/rvcr/ and https://docs.rs/reqwest-vcr/latest/reqwest_vcr/
  - Both are described with the same sentence, "Record-and-replay middleware for reqwest http client", and show the same API: `VCRMiddleware::try_from(path)`, `.with_mode(VCRMode::Record)` or `VCRMode::Replay`, plugged in through `reqwest_middleware::ClientBuilder`.
  - They sit inside the client as middleware, so the system under test must build its `reqwest` client through `reqwest-middleware`.
  - Streaming behavior during replay stays unconfirmed. I read only the crate front pages.
- SSE fixture helpers seen in the wild:
  - Codex `sse(events)` plus `ev_*` builders (Responses API events), and `StreamingSseChunk` with a oneshot gate per chunk.
  - Goose `.txt` files of raw recorded chat-completions SSE, loaded with `include_str!`.
  - opencode `packages/simulation/src/backend/openai.ts`, which frames protocol items as chat-completions chunks.
  - OpenHands `_send_streaming`, which re-cuts a complete chat-completions message into chunks.
  - The opencode tree also lists `packages/ai/test/lib/sse.ts` and `openai-chunks.ts`, which I did not open.

## What I could not confirm

- Codex contributor docs on testing: `AGENTS.md` at the repo root and under `codex-rs` both returned 404 today, so the conventions above come from test code only.
- Goose scenario tests, and the bodies of the thinking-related providers in `agent.rs` (names seen, code not read). The MCP replay files were seen by path only.
- Zed's executor internals, and a type named `FakeLanguageModel`. The fake-clock behavior is taken from how the tests call `run_until_parked` and `advance_clock`.
- Crush cassette format and matching rules.
- Continue tests that use `MockLLM` or `TestLLM`.
- Claude Code internals, and the Claude Agent SDK `e2e-tests` directory.
- Kiro CLI, which the Amazon Q README describes as closed source. The variable name `Q_MOCK_CHAT_RESPONSE` comes from a search-result excerpt.
- Letta's archived V1 server tests. I read only the current `letta-code` fake backend, and I did not open the tests that use it.
- opencode's `parity/lib/fake-server.ts`, `parity/sim/backend.ts`, and `packages/ai/test/lib/sse.ts` - described from the testing doc and file names.
- Streaming replay behavior of `rvcr` and `reqwest-vcr`, and whether `httpmock` or `httptest` can stream a body in timed chunks through some API beyond the pages I read.
- A third write-up (weft docs on record and replay with request hashing and golden files) appeared in search results only. I did not open the page.

## Patterns across projects

### Three places to cut

- In-process fake behind the model trait: Zed, Pi, Gemini CLI, OpenHands `TestLLM`, SWE-agent, mini-swe-agent, smolagents, Continue, Amazon Q, Letta, the hand-written Goose providers, Pydantic AI, Vercel AI SDK. Fast and simple, and it skips the HTTP client, SSE parser and retry code.
- Local HTTP server with fixtures: Codex (wiremock plus a custom streaming server), Goose `OpenAiFixture`, Cline mock server, OpenHands mock LLM server, opencode simulation. The real client stack runs, so wire-format, retry and stream-cut bugs are reachable.
- Recorded traffic: Crush (VCR cassettes), Goose `TestProvider`, Gemini CLI goldens, opencode `http-recorder`, Cline VCR, SWE-agent `ReplayModel`. Realistic payloads, at the price of cassettes that go stale when prompts change.
- The larger projects use two or three of these at once, one per test tier.

### Script shapes

- Ordered queue of whole responses, one per model call. This is the most common shape (Codex `mount_sse_sequence`, Pi, OpenHands, mini-swe-agent, SWE-agent, Gemini CLI, Amazon Q, Vercel AI SDK).
- Queue entries with a request check attached: Goose pairs each response with a substring the request must contain, and Codex has `mount_sse_once_match`.
- Response chosen by request content, with no order: Codex's 500-versus-200 test by body substring, Goose `TestProvider` by message hash, Cline's mock server by keyword, Continue `MockLLM` by count of user and tool messages, smolagents by transcript markers.
- Hand-driven streams: Zed and opencode park each model call as a pending request, and the test pushes events and ends the stream itself. This gives full control of interleaving and is the only shape that tests "something happens while the model is still streaming" cleanly.
- Event builders over raw JSON: Codex `ev_*`, Pi `fauxText` / `fauxThinking` / `fauxToolCall`, mini-swe-agent `make_output`. They keep scripts short and survive wire-format changes.
- Scripts in files for black-box runs of the built binary: Gemini CLI (JSON lines plus a CLI argument), Amazon Q (JSON file plus an environment variable), opencode scenario files.
- Scripts loaded over an admin API at run time: OpenHands (`/admin/trajectory/register`), Cline (`/.test/...`), for browser-driven suites where the server outlives one test.

### How each behavior is simulated

- Tool-call round: one response that ends in a tool call, then one that ends in text. Every project does this the same way.
- Parallel tool calls: several tool-call items in one response (Codex, Gemini CLI). Codex adds a test-only tool with a barrier to prove the calls overlapped.
- Streaming deltas: explicit delta events (Codex, Zed, opencode), or automatic chunking of a whole message (Pi by token size, Cline by word, OpenHands by part).
- Chunk timing: gates released by the test (Codex), manual sends plus a fake clock (Zed), a tokens-per-second setting (Pi), a per-chunk delay (Cline, Vercel `simulateReadableStream`).
- Reasoning: dedicated builders or items (Codex `ev_reasoning_item`, Pi `fauxThinking`, opencode `reasoningDelta`, Zed `Thinking` event).
- Errors in the script: exception objects in the queue (OpenHands, mini-swe-agent), magic strings (SWE-agent `raise_context`, Continue `"ERROR"`), HTTP status entries (Codex `ResponseTemplate::new(429)`, Goose `status:<code>`), a failed-response event (Codex `sse_failed`).
- Retries and rate limits: a failing entry followed by a good one in the same queue, then an assertion on request count (Codex), or a fake clock advanced past the backoff (Zed).
- Cut streams: a body that ends before the completion event (Codex), an error pushed after partial text (Zed), `disconnect(id)` (opencode), a write callback that returns an error (mockito).
- Context-window overflow and cost limits: scripted exceptions (OpenHands, SWE-agent), scripted token counts (Codex `ev_completed_with_tokens`, Zed `queue_input_token_count`), computed usage (Pi estimates tokens from text length).

### What tests assert on

- The requests the harness sent. Codex (`ResponseMock`, `ResponsesRequest` helpers), Zed (`pending_completions()`), OpenHands (`GET /admin/requests`), Vercel (`doGenerateCalls`), Pydantic AI (`capture_run_messages`). This is where message assembly, tool-result routing, prompt caching flags and compaction get checked.
- The final transcript or stored messages (Zed `to_markdown()`, Crush message store, Pi session messages).
- The event stream the harness emits (Codex `EventMsg`, Goose `AgentEvent`, Pi `AgentSessionEvent`).
- Side effects such as files on disk.
- Snapshots of the model-visible context with volatile parts normalized (Codex `context_snapshot`).
- A fake that reports on the request in its reply, so a black-box test can assert on plain output: Continue `REPEAT_SYSTEM_MSG`, Cline's hook-context probe, Letta's `tool result received (...)` text, Goose's second-round pattern on the tool result.

### How determinism is enforced

- Overrun is loud in most projects: Codex panics (`no response for N`), Pi returns `No more faux responses queued`, OpenHands raises `TestLLMExhaustedError`, Gemini CLI throws `No more mock responses`, Goose answers 417, the Codex streaming server answers 500. Amazon Q is the exception and returns an empty reply.
- Underrun is checked less often: Codex uses `.expect(n)`, opencode's recorder fails on unused interactions, httptest panics on drop, Pi and OpenHands expose a remaining count for the test to check.
- Request invariants checked inside the fake on every call (Codex's call-and-output pairing check).
- Fake or controlled time: Zed's executor (`run_until_parked`, `advance_clock`), gates and signals in Codex plus a paused tokio clock for its backoff unit tests, "wait with `until`, not with time" in opencode, counter-based timestamps in Letta, a fixed time function in Crush.
- A global block on real model calls: Pydantic AI `ALLOW_MODEL_REQUESTS = False`, Zed `forbid_requests()`, opencode's `CI=true` rule that a missing cassette fails.
- Side calls are the usual source of flakiness (title generation, routing, summarizing, preflight pings). Projects handle them with a rule beside the script (OpenHands ping, opencode fixed title), a non-strict queue mode (Gemini CLI), or by turning the feature off in test config (Goose disables session naming "so it doesn't consume a provider call").
- Volatile fields are normalized before comparison: ids, timestamps, paths, ports (Codex, opencode, Cline, Pydantic AI matchers).

### Stand-ins that generate content by rule

- Fixed reply: Letta `DeterministicPongExecutor` ("pong"), Continue `TestLLM` chat, Continue `MockLLM` default.
- Echo: Continue `MockLLM` `REPEAT_LAST_MSG` and `REPEAT_SYSTEM_MSG`, Continue `TestLLM` `PROMPT: <prompt>`.
- Keyword or marker rules: Cline mock server, Continue `TestLLM` numbered messages, smolagents fake models, Letta `DeterministicReflectionExecutor`.
- State machine on conversation shape: Letta `DeterministicToolCallExecutor` (tool call first, then a summary of the tool result), SWE-agent `InstantEmptySubmitTestModel`, Goose `MockToolProvider`, smolagents `FakeToolCallModel`.
- Schema-driven generation: Pydantic AI `TestModel` calls every tool with arguments built from its JSON schema.
- Programmable step: Pi factory steps, Pydantic AI `FunctionModel`, Codex `mount_response_handler`, httpmock `respond_with`, mockito `with_body_from_request`.
- Random scripts for property tests: described in the n4n write-up only. The agent repos I opened all use fixed scripts or fixed rules.
- What they are used for: smoke tests and end-to-end UI tests where the exact reply does not matter, loops that must end by themselves (max turns, submit), proving a round trip through a real tool, and long-lived fake servers shared by many tests.

### What this suggests for a Rust fake LLM that speaks chat completions

- Keep the scripted in-process model for loop logic and add an HTTP-level fake for the gateway and client code. Confidence: high - Goose runs both levels side by side, Codex tests almost everything at the HTTP level, and Zed and Amazon Q show how far the in-process level goes.
- Make the HTTP fake's core a per-request queue with optional request matchers, event builders for chat-completions chunks, recorded requests with typed accessors, and a panic on overrun and on unconsumed entries. Confidence: high - this is the common core of Codex, Goose and OpenHands.
- Serve streams from a small custom server with a gate per chunk rather than from wiremock alone, when tests need mid-stream timing or cut streams. Confidence: medium - Codex took this route, and mockito's `with_chunked_body` is a ready-made alternative I have read about but not run.
- Add a rule mode (echo, keyword to tool call, tool-result summary) for long-lived and black-box runs, and keep side calls such as titles and pings out of the script with explicit rules. Confidence: medium - Cline, Letta and OpenHands show the value, but each built it ad hoc.
- Check request invariants inside the fake (every tool result has a call, every call has a result). Confidence: high - cheap, and Codex gets it on every test.
