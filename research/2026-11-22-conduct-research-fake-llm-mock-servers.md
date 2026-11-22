---
produced: 2026-11-22
title: "Fake LLM mock servers survey - standalone OpenAI chat completions, Responses API and Anthropic Messages mock servers with zero model inference (aimock/llmock, VidaiMock, llmposter, openai-mock-server, mock-llm, llm-mock-server, LLMock, AI-Mocks/Mokksy, llm-d-inference-sim, MockGPT) - reply selection, SSE streaming, tool calls, chaos and error injection, latency simulation, token usage, test control, Rust crates"
---

# Fake LLM mock servers - what exists and how each one works

This is a survey of standalone servers and libraries that imitate the OpenAI API (chat completions, Responses API) or the Anthropic Messages API with a rule engine or a text generator standing where the model would be. The goal is to learn what to copy for a fast, local, deterministic fake LLM that tests a Rust agent harness and a gateway speaking OpenAI chat completions (JSON and SSE, with tool calls).

## How I checked things

- Date of all checks: 2026-11-22.
- Every fact below was read on a page I fetched that day: a README, a docs page, a docs.rs page, or a registry API response. Each project section ends with the URLs.
- Repository metadata (language, license, last push, stars, archived flag) is from the GitHub REST API. Package versions and dates are from the npm registry, the crates.io API and the PyPI JSON API.
- Where a point is marked "unconfirmed", I looked for it and the pages I opened did not settle it.
- Feature claims are the projects' own claims. I read docs and source snippets. I did not install or run any of these.

## The short list

Ten projects get a full section because they are closest to our need:

- aimock (CopilotKit, formerly llmock) - TypeScript - fixture matching with the richest feature set of anything I found.
- VidaiMock - Rust - zero-config server where every reply is a template, with a built-in rule that ends tool loops.
- llmposter - Rust - fixture-driven, in-process or CLI, seeded chaos, named state machines for multi-turn.
- openai-mock-server (genai-rs) - Rust - scripted reply queue per operation, typed against OpenAI's OpenAPI spec.
- mock-llm (dwmkerr) - TypeScript - YAML rules with JMESPath matching and response templates.
- llm-mock-server (theblixguy) - TypeScript - `when(...).reply(...)` rules, reply sequences, retry-aware.
- LLMock (enrico9034) - Go - lorem ipsum at an exact token count, steered by directives written inside the prompt.
- LLMock (piyook/llm-mock) - TypeScript - response rules plus lorem ipsum or a pool of stored replies for everything else.
- AI-Mocks on Mokksy - Kotlin - request-spec DSL for JVM tests, true SSE streaming.
- llm-d-inference-sim - Go - vLLM simulator with echo and random-sentence modes and modeled latency.

Shorter notes follow for mockllm (Stacklok), mock-openai-api (zerob13), mock-openai-server (freakynit), mockai (polly3d), MockAI (ajac-zero), FakeAI, openai-responses-python, WireMock MockGPT and WireMock Cloud, the `llm-mock` Rust crate, and MSW.

## 1. aimock (CopilotKit, formerly llmock)

- Identity: The lead `CopilotKit/llmock` is real. The GitHub API resolves that name to `CopilotKit/aimock`, so the repo was renamed. The README says the npm package was renamed from `@copilotkit/llmock` to `@copilotkit/aimock` at v1.7.0 and the main class is still called `LLMock`.
- Language: TypeScript on Node.js. The README says it uses Node builtins only, with zero runtime dependencies.
- License: MIT.
- Signs of life: npm `@copilotkit/aimock` 1.48.1 published 2026-11-20. GitHub release v1.48.1 on 2026-11-20. Last push 2026-11-20. 1031 stars. Repo created 2026-03-03.
- How it decides what to say: Fixtures. Each fixture has a `match` and a `response`. All match fields are AND-ed and the first fixture that passes wins, in registration order (files from a directory load in sorted filename order). Match fields I saw: `userMessage` (the last user message only; substring by default, a RegExp is allowed, and it flips to exact equality when a `requestTransform` is set), `systemMessage`, `model`, `toolName` (a tool offered in the request), `toolCallId` (the last message is a tool result with this id), `toolResultContains`, `responseFormat`, `sequenceIndex` (Nth hit of this pattern, a server-side counter), `turnIndex` (count of assistant messages in the request, derived from the request alone), `hasToolResult` (a tool result exists after the last user message, also derived from the request alone), `endpoint`, `context` (the `X-AIMock-Context` header), and `predicate` (a function, programmatic only). A response can also be a sync or async function of the request.
- Unmatched requests: 404 by default. With the `strict` flag an unmatched request gets 503. In record mode or proxy-only mode it is forwarded to the real provider.
- Fixture linting: `validateFixtures()` runs at load and warns about a duplicate `userMessage` that shadows an earlier fixture and about a catch-all that sits before other fixtures. There is also an offline `aimock validate` command.
- Record and replay: It can proxy a real API, save the replies as fixture files, and replay them. Recordings keep per-frame arrival timestamps and the final usage frame.
- Streaming: SSE. `chunkSize` is characters per SSE chunk and `latency` is milliseconds between chunks. A `streamingProfile` of `{ ttft, tps, jitter }` gives time to first token, then a delay of `1000 / tps` ms per chunk, and each delay is multiplied by `1 + random(-1,1) * jitter`. With `stream_options.include_usage` it sends a final usage chunk. A `blocks` array sets the order of text and tool calls in the stream (tool-first or interleaved).
- Tool calls: A response can hold `toolCalls: [{ name, arguments, id? }]`, with or without text. If the fixture omits the id, a fresh id is generated per reply, so a follow-up fixture keyed on `toolCallId` needs a pinned id. The follow-up round is matched with `toolCallId`, `hasToolResult`, `turnIndex`, `toolResultContains` or `sequenceIndex`. The docs recommend the request-derived fields (`turnIndex`, `hasToolResult`) when several test runners share one server, because `sequenceIndex` counters drift under concurrency.
- Model misbehavior: A separate `misbehavior` field on a fixture injects semantic faults after the fixture is chosen, for example `tool-args-invalid-json` and `stop-length-mid-tool`. The README also lists schema violations, unknown tools, duplicate ids, empty responses, refusals, content filtering and reasoning-only responses. Faults can be seeded and limited with `times`. The two pages disagree on reach: the fixtures page says only the OpenAI chat wire renders faults, while the README lists more wires and says support varies by wire.
- API shapes: OpenAI Chat Completions, Responses API (HTTP and WebSocket), Realtime, Anthropic Messages, Gemini, Bedrock, Azure, Vertex AI, Ollama, Cohere, OpenRouter. Also embeddings, images, speech, transcription, video, plus MCP, A2A, AG-UI and vector database mocks.
- Error and chaos injection: `nextRequestError(status, body)` queues a one-shot error for the next request. A fixture can return an error with a status. `truncateAfterChunks` aborts a stream after N chunks. `disconnectAfterMs` kills the connection on a timer. Chaos has four probabilistic modes: drop (HTTP 500), malformed (HTTP 200 with a broken JSON body), rateLimit (HTTP 429 with `Retry-After`), disconnect (TCP connection destroyed), plus a fixed `latencyMs`. Chaos resolves at three levels: per-request headers (`x-aimock-chaos-drop` and friends) over fixture config over server defaults. Runtime changes go through `/__aimock/chaos`, scoped to the caller's `X-Test-Id`.
- Latency simulation: `ttft`, `tps`, `jitter` as above. `nonStreamingTiming` applies the same total delay to a non-streamed reply. Replay can use recorded timings with a speed multiplier.
- Token usage: Auto-computed from content length when the fixture leaves it out. A fixture can override `usage`. Recorded fixtures replay the real recorded usage. The token-count endpoints return an estimate of about 1 token per 4 characters.
- How a test controls it: In-process `new LLMock({ port: 0 })` with `start()` and `stop()`, plus `on`, `onMessage`, `onToolCall`, `onToolResult`, `onTurn`, `addFixture`, `reset()`. JSON fixture files and directories. A CLI and a Docker image. An HTTP control API under `/__aimock/` (fixtures, chaos, error, journal). Vitest and Jest plugins that start the server, patch `OPENAI_BASE_URL`, and reset match counters before each test.
- Request recording: A journal of every request at `GET /__aimock/journal`, filterable by path, status, `testId` and request id. Entries record which fixture matched, any chaos action, and whether the reply was interrupted (including a client that hung up).
- Extras worth knowing: A daily drift-detection CI job compares its output shapes with the real APIs.
- Sources:
  - https://github.com/CopilotKit/aimock
  - https://raw.githubusercontent.com/CopilotKit/llmock/main/README.md
  - https://aimock.copilotkit.dev/fixtures
  - https://aimock.copilotkit.dev/multi-turn
  - https://aimock.copilotkit.dev/sequential-responses
  - https://aimock.copilotkit.dev/streaming-physics
  - https://aimock.copilotkit.dev/chaos-testing
  - https://aimock.copilotkit.dev/error-injection
  - https://aimock.copilotkit.dev/record-replay
  - https://aimock.copilotkit.dev/test-plugins

## 2. VidaiMock

- Identity: Real. A Rust mock server from Vidai, also usable as a library inside Rust tests.
- Language: Rust (Axum, Tokio, Tera templates).
- License: Apache-2.0.
- Signs of life: GitHub release v0.4.0 on 2026-10-27. Last push 2026-10-27. 45 stars. Repo created 2025-12-31. The crates.io API showed `vidaimock` at 0.3.1 (updated 2026-08-04), while the README shows `vidaimock = "0.4"` in its example, so 0.4 on crates.io is unconfirmed. The docs say the library API is new in 0.3.0 and may change between minor versions.
- How it decides what to say: Templates, with zero fixtures by default. Every response is a Tera template rendered from the request (`json`, `headers`, `query`, `path_segments`). The bundled OpenAI chat template branches like this: tools are declared and a tool result is already in the history gives a plain-text answer ("Based on the tool results...") with `finish_reason: "stop"`; tools are declared and the history is still waiting on a tool result gives a tool call that echoes the name of the first declared tool with arguments `"{}"`; a `response_format` gives JSON-string content; a model name starting with "o" adds reasoning tokens to usage; anything else gives default assistant text. The docs describe the server as stateless: each request is a pure function of the request and the loaded config.
- Customizing replies: Provider YAML files (path `matcher` regex, `priority`, `response_template`, `error_template`, `status_code` as a static value or a Tera expression) and templates live in a config directory that overlays the embedded defaults. An isolated mode loads only your own directory.
- Streaming: The engine renders the full response, pulls out the streamable content, and splits text into word chunks. Each chunk goes through `on_start`, `on_chunk`, `on_stop` templates (with `chunk`, `chunk_index`, `chunk_count` in scope). A tool call is sent as one structured chunk, never word-split. The default `benchmark` mode sends chunks as fast as possible. `realistic` mode adds time to first token (the base latency) and paces the following tokens, with `jitter_pct` for variance. It honors `stream_options.include_usage` with a final usage chunk. The docs claim the SSE framing is regression-tested byte for byte against captured real-provider output.
- Tool calls: Automatic, as described above. The `has_tool_result(messages, provider)` helper detects an OpenAI `role: "tool"` message, an Anthropic `tool_result` block, or a Gemini `functionResponse` part. That is how an agent loop ends on its own against this mock.
- API shapes: OpenAI chat completions, Responses API (typed SSE events), legacy completions, embeddings, images, audio, videos, moderations. Anthropic Messages (all seven SSE event types) and count_tokens. Gemini, Azure OpenAI, Vertex AI, Bedrock (binary event stream), and OpenAI-compatible Cohere, Mistral, Groq.
- Error and chaos injection: `X-Mock-Status: 429` forces a status on any real route. `?chaos_status=503` does the same per URL. `X-Vidai-Chaos-Drop` is a probability of a provider-shaped 500. `X-Vidai-Chaos-Malformed` is a probability of a broken JSON body. `X-Vidai-Chaos-Disconnect` is a probability of a cut stream. `X-Vidai-Chaos-Trickle` is a per-chunk delay. The same settings exist in a `[chaos]` config block. An `/error/{code}` route returns any status. All errors go through the provider's `error_template`, so the body has the provider's error envelope. It also validates some required fields (an Anthropic request with `max_tokens` missing gets a 400).
- Latency simulation: A base latency flag and a `latency_ms` builder option, `realistic` mode, `X-Vidai-Latency` and `X-Vidai-Jitter` per request.
- Scenario playback: For exact scripted replies, a provider can replay scenario files picked per request, by the `X-Vidai-Scenario` header or by a key rendered from the request (their example derives it from the model name). An `.sse` file is sent as written, and each blank-line-separated event is its own network write. Two comment lines script the timing: `:vidai-delay 250` waits before sending an event, and `:vidai-split` ends the current network write in the middle of an event. A scenario id whose files are missing returns a 500 that names the id, so a mis-wired test fails loudly.
- Token usage: Filled by the templates by default. The docs list a `random_int(min, max)` helper "for mock token counts", and the exact numbers the bundled templates produce are unconfirmed. A test can pin exact numbers with the `X-Vidai-Usage` header (`input=..,output=..,cached=..,cache_write=..,reasoning=..`), rendered in each provider's own usage shape. `X-Vidai-Output-Tokens: n` generates n words of output, one per stream chunk, and reports n output tokens; the value `max` uses the request's own max-tokens field.
- Determinism note: Templates use `uuid()` and `timestamp()` helpers for ids and created times, so those fields differ from run to run. Chaos is a probability roll and I saw no seed setting.
- How a test controls it: In-process `MockServer::builder().bind("127.0.0.1:0").config_dir(..).isolated(..).latency_ms(..).mode(..).start().await`. The server is ready when `start()` returns. `addr()`, `base_url()`, `shutdown()`. Instances share no global state, so each test can own one. Per-request control is by headers. The `MockServer` docs.rs page lists exactly `builder`, `addr`, `base_url` and `shutdown`, so request recording and assertions are left to the test.
- Sources:
  - https://github.com/vidaiUK/VidaiMock
  - https://docs.vidai.uk/mock/
  - https://docs.vidai.uk/mock/templating/
  - https://docs.vidai.uk/mock/reference/architecture/
  - https://docs.vidai.uk/mock/streaming/
  - https://docs.vidai.uk/mock/getting-started/cli-reference/
  - https://docs.vidai.uk/mock/recipes/rust-library/
  - https://docs.rs/vidaimock/latest/vidaimock/struct.MockServer.html
  - https://crates.io/crates/vidaimock

## 3. llmposter

- Identity: Real. A Rust crate from SkillDoAI, usable as a library in tests or as a CLI binary.
- Language: Rust (axum). MSRV 1.89.
- License: AGPL-3.0 (crates.io says AGPL-3.0-or-later). This matters if we ever want to depend on it or borrow code.
- Signs of life: crates.io 0.5.0 updated 2026-07-07. GitHub release v0.5.0 on 2026-07-07. Last push 2026-10-17. 3 stars. Repo created 2026-03-14.
- How it decides what to say: Fixtures, in YAML or through a builder. Match fields: `user_message` (substring of the last user message by default, or `regex:`), `model`, `headers`, `system_prompt`, `temperature` (exact or range), `metadata`, `tool_schema` (name of a declared tool), and `body_jsonpath` (RFC 9535 JSONPath over the whole body). Ordering is by descending `priority`, then file order, first match wins, then a separate pass over fixtures marked `catch_all: true`. A request that matches nothing gets a 404. Fixtures are validated at load.
- Multi-turn: Named scenarios. A fixture can say `required_state` and `set_state` for a named state machine. Two fixtures with the same `user_message` then answer the first and second call differently (tool call first, text second), and the same trick models "429 first, success on retry". State lives per server instance and `server.reset()` clears it.
- Templating: An optional `content_template` renders a Jinja-style template with `user_message`, `model`, `provider`, and the full `request`.
- Record and replay: VCR modes `Replay`, `Record`, `RecordOnMiss`, with regex redaction of recorded content.
- Streaming: SSE. `streaming.latency` is milliseconds between chunks and `streaming.chunk_size` is characters per chunk. The OpenAI stream is a role chunk, content chunks, a stop chunk, then `data: [DONE]`. Tool-call arguments go out in a single frame, whatever the chunk size.
- Tool calls: `tool_calls: [{ name, arguments }]` where arguments must be a JSON object, checked at load. Tool-call ids are globally unique across requests. The follow-up round is handled with scenarios.
- API shapes: OpenAI chat completions, Anthropic Messages, OpenAI Responses API, Gemini (both streaming modes), legacy completions, embeddings, moderations, models. One fixture serves every provider route unless it is pinned with `provider:`.
- Error and chaos injection: `error:` fixtures with any status from 400 to 599, the provider's error envelope, and custom headers such as `retry-after`. `failure:` on a normal response: `latency_ms`, `corrupt_body` (plain text "overloaded"), `truncate_after_frames`, `disconnect_after_ms`. Seeded streaming chaos: `latency_jitter_ms`, `duplicate_frames`, `probability`, `chaos_seed`. Same seed and same request order give the same chaos every run. A `refusal:` fixture returns each provider's native refusal shape (non-streaming only as of v0.4.5).
- Latency simulation: `failure.latency_ms` before the response, `streaming.latency` between chunks, seeded jitter.
- Token usage: A `bytes / 4` estimate. Their docs say to assert that counts are positive and that total equals prompt plus completion.
- Determinism: Response ids are sequential per server (`chatcmpl-llmposter-1`, `chatcmpl-llmposter-2`). `system_fingerprint` is always `fp_llmposter`.
- How a test controls it: `ServerBuilder::new().fixture(..).build().await` starts an axum server on an OS-assigned port, one per test, stopped on drop. `server.url()`, `get_requests()`, `request_count()`, `scenario_state(name)`, `reset()`, `set_fixtures(vec)` to swap fixtures mid-test. Each captured request has an `outcome`: `Matched`, `NoFixtureMatch`, `BadRequest`, `AuthRejected`, `CodeEndpoint`. Bearer-token auth with a use count is available for testing 401 paths.
- Known gaps they document: `logprobs` is always null, the Responses stream covers the core text and function-call events only, and most request fields are accepted and ignored.
- Sources:
  - https://github.com/SkillDoAI/llmposter
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/fixtures.md
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/scenarios.md
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/failure-simulation.md
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/request-capture.md
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/library.md
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/providers/openai.md
  - https://raw.githubusercontent.com/SkillDoAI/llmposter/main/docs/spec-deviations.md
  - https://crates.io/crates/llmposter

## 4. openai-mock-server (genai-rs)

- Identity: Real, and very new. The crate was created on 2026-11-18, four days before this survey.
- Language: Rust (axum). MSRV 1.88.
- License: MIT OR Apache-2.0.
- Signs of life: crates.io 0.2.5 updated 2026-11-20. Last push 2026-11-21. 0 stars, 434 downloads.
- How it decides what to say: A scripted queue per API operation. `mock.on(op::CREATE_CHAT_COMPLETION).reply(a).reply(b)` answers the first call with `a` and the second with `b`, then falls back to the `always` reply if one is set. It does zero content matching: order is the whole script. An operation with nothing scripted answers 501.
- The distinctive idea: The server and its models are generated from OpenAI's OpenAPI specification. A request that fails to decode, or that breaks a validation keyword in the spec (`minimum`, `maxLength`, `pattern`), gets a 400 and shows up in `mock.rejected()` with the path of the bad value. A scripted 200 reply that fails to decode as the operation's response model gets a 500 that names the operation, so a wrong fixture fails the test loudly.
- Streaming: `Reply::stream(Stream::new(fixtures::chat_chunks(&["Hel", "lo"])))`. The test supplies the text pieces and each piece becomes one chunk, after a role chunk and before a finish chunk. Stream options: `interval(Duration)` waits before every event, `keep_alive()` writes a `: keep-alive` comment before every event, `event_names(bool)` turns `event:` lines on or off, `without_done()` ends the stream with `[DONE]` left out (as a cut stream does), `error_after(n, StreamError)` sends an error event after n events and ends the stream.
- Tool calls: `fixtures::chat_tool_call(name, arguments)` builds a non-streamed chat completion with one tool call (id `call_mock`, `finish_reason: "tool_calls"`). `fixtures::response_function_call` does the same for the Responses API. The follow-up round is the next reply in the queue. The fixtures module's streamed chat helper is text only, so a streamed tool call is built by hand from `serde_json` values.
- API shapes: Chat completions and Responses (JSON and streams), embeddings, models, files, batches, images, audio, moderations, realtime calls, and signed webhooks. OpenAI only.
- Error and chaos injection: `Reply::error(status, message)` in OpenAI's error envelope with the type chosen from the status. `Reply::rate_limited(retry_after)` gives a 429 with `retry-after` and `retry-after-ms`. `.delay(Duration)` waits before answering, for timeout and cancellation tests. `.status()` and `.header()` adjust any reply. Stream faults are listed above.
- Latency simulation: `Reply::delay` and `Stream::interval`. Everything is explicit and fixed.
- Token usage: Constants in the fixtures: `prompt_tokens: 9`, `completion_tokens: 12`, `total_tokens: 21`. The fixture is a `serde_json::Value`, so a test can edit fields before scripting it.
- Determinism: Fixed id `chatcmpl-mock`, fixed `created` timestamp (2024-01-01), fixed model name in fixtures.
- How a test controls it: `MockServer::start().await` binds `127.0.0.1` on a free port and stops when dropped. `mock.base_url()`. `mock.requests::<CreateChatCompletionRequest>()` returns the recorded requests as typed structs with headers. `MockServer::with_api_key` answers 401 when the bearer token is missing.
- Sources:
  - https://github.com/genai-rs/openai-mock-server
  - https://crates.io/crates/openai-mock-server
  - https://docs.rs/openai-mock-server/latest/openai_mock_server/
  - https://docs.rs/openai-mock-server/latest/openai_mock_server/struct.Stream.html
  - https://docs.rs/openai-mock-server/latest/openai_mock_server/struct.Reply.html
  - https://docs.rs/openai-mock-server/latest/src/openai_mock_server/fixtures.rs.html

## 5. mock-llm (dwmkerr)

- Identity: Real. A Node server configured with a YAML file, built for Kubernetes integration tests (the Ark project uses it).
- Language: TypeScript.
- License: MIT.
- Signs of life: GitHub release v0.1.29 on 2026-04-22, npm `@dwmkerr/mock-llm` 0.1.29 the same day. Last push 2026-07-28. 14 stars. The README's install line uses the unscoped name `mock-llm`; the npm registry shows a package with that name at 0.2.3 (2026-11-03), and I did not verify that it is the same project.
- How it decides what to say: By default it echoes the last user message. Beyond that, YAML rules. Each rule has a `path` regex, a `method`, an optional `match` written as a JMESPath expression over the request (for example `contains(body.messages[-1].content, 'hello')`), an optional `sequence` number, and a `response` with a status and a content template. The last matching rule wins. The response content is the full JSON body written by hand, with `{{jmes request body.model}}` style lookups and `{{timestamp}}`.
- Streaming: When the client sends `stream: true` it returns SSE. Chunking is global config: `chunkSize` characters per chunk (default 50) and `chunkIntervalMs` between chunks (default 50).
- Tool calls: You write the `tool_calls` JSON yourself in the response body. A tool round trip uses `sequence: 0` for the tool call and `sequence: 1` for the final answer. Counters are per path and reset with `DELETE /config`. Rules with no `sequence` do not consume a counter, which keeps health probes from eating the script.
- API shapes: OpenAI chat completions and `/v1/models`, plus MCP and A2A mocks and an OAuth emulation for MCP. Responses API coverage is unconfirmed: the tagline mentions it and the intro says the server is "currently designed to mock the OpenAI Completions API".
- Error and chaos injection: Any status and body through a rule (the README shows a realistic 401). The README says error conditions in streams follow the OpenAI streaming spec. I saw deterministic rules only.
- Latency simulation: The chunk interval. Other delay settings are unconfirmed.
- Token usage: Whatever the rule's template says. Their sample hardcodes 10, 20 and 30.
- How a test controls it: The YAML file, or the runtime `/config` endpoint (`GET` reads, `POST` replaces, `PATCH` merges, `DELETE` resets). Docker image and Helm chart. Port 6556. Request recording is unconfirmed.
- Sources:
  - https://github.com/dwmkerr/mock-llm
  - https://raw.githubusercontent.com/dwmkerr/mock-llm/main/README.md
  - https://raw.githubusercontent.com/dwmkerr/mock-llm/main/samples/09-token-usage.sh

## 6. llm-mock-server (theblixguy)

- Identity: Real. A small Fastify server with a fluent rule API, used by its author's proxy projects for integration tests.
- Language: TypeScript.
- License: MIT.
- Signs of life: npm 1.1.0 published 2026-06-12, GitHub release v1.1.0 the same day. Last push 2026-07-25. 5 stars. Repo created 2026-03-14.
- How it decides what to say: Rules in order, first match wins. `server.when("hello").reply("Hi!")` is a case-insensitive substring match on the last user message. A matcher can also be a regex, an object (`model`, `system`, `format`, `message`, plus an optional `predicate`), or a plain predicate function. A reply can be a string, an object (`{ text, reasoning }` or `{ tools: [...] }` or `{ error: {...} }`), or a sync or async function of the request. `server.fallback(...)` sets the reply for requests that match nothing.
- Sequencing: `.times(n)` limits a rule to n matches and `.first()` moves it to the front. `replySequence([...])` walks through a list of replies, one per match, then stops matching.
- Retry awareness: A retried request replays the previous reply and does not advance the sequence. It detects retries from the `x-stainless-retry-count` header that the OpenAI and Anthropic SDKs send.
- Streaming: SSE on all three routes. `latency` (ms between chunks) and `chunkSize` (characters per chunk) per rule or as server defaults.
- Tool calls: A reply of `{ tools: [{ name, args }] }`. `whenTool("get_weather")` matches on an offered tool name. `whenToolResult("call_abc")` matches the follow-up round by tool call id.
- API shapes: OpenAI chat completions, Anthropic Messages, OpenAI Responses API. One rule serves all three. Internally each format has a parser into a common request shape and a serializer from a common reply shape.
- Error and chaos injection: `server.nextError(429, "Rate limited")` is one-shot. A rule can reply with an error and status. A closure-based matcher gives "fail every third request". I saw explicit rules only.
- Token usage: Unconfirmed.
- How a test controls it: In-process `createMock()` on port 0, stopped by `await using` or `stop()`. `server.history` records every request with `count()`, `first()`, `last()`, `where()`, including which rule matched. `server.rules` shows remaining match counts and `server.isDone()` says whether all limited rules were used up. Rules can also load from JSON5 files (with named reply templates and sequences) or TypeScript handler files, with a watch mode.
- Sources:
  - https://github.com/theblixguy/llm-mock-server
  - https://raw.githubusercontent.com/theblixguy/llm-mock-server/main/README.md

## 7. LLMock (enrico9034)

- Identity: Real. A different project from CopilotKit's old "llmock" name. A Go server built for load-testing whatever sits in front of an LLM (LiteLLM, gateways, rate limiters).
- Language: Go. One static binary with one dependency (the Prometheus client).
- License: MIT.
- Signs of life: Repo created 2026-10-27 and last pushed the same day. 3 stars. The GitHub releases endpoint returned 404, so I saw commits and a container image reference only.
- How it decides what to say: Lorem ipsum, at exactly the token count you ask for. The request itself says how it should be answered, through a directive block inside the prompt: `Hello [[mock tokens=50 tps=10]]`. The block can sit anywhere in the system prompt or the last user message. The same directives can come from an `X-Mock-Directives` header or from server-side defaults and overrides. Layering from lowest to highest: server defaults, header, prompt, server overrides. Unknown keys and bad values get a 400.
- Why directives live in the prompt: The README says it works through proxies that drop custom headers (it names LiteLLM as one that drops the header and passes the prompt through).
- Directive keys: `tokens` (exact number or a range), `prompt_tokens`, `delay` (time to first token), `tps`, `jitter`, `chunk` (tokens per stream frame), `error` (HTTP status), `error_rate`, `error_after` (in-stream error after N tokens), `fail_after` (drop the connection after N tokens), `hang` (never send headers), `hang_after` (go silent after N tokens), `finish_reason` (`stop`, `length`, `content_filter`), `seed` (deterministic text and ranges).
- Plan echo: Every generated response has an `X-LLMock-Plan` header listing the parameters it was played with.
- Streaming: SSE for OpenAI and Anthropic formats, one token per frame by default, paced on an absolute schedule so timing does not drift under load. With `stream_options.include_usage` it sends the usage chunk before `[DONE]`.
- Tool calls: The directive table covers length, timing and failures. Replies are lorem ipsum text, and tool-call replies are unconfirmed.
- API shapes: OpenAI chat completions, legacy completions, embeddings (deterministic unit vectors), models. Anthropic Messages, count_tokens, models.
- Error and chaos injection: The directives above, plus a global rate limit and a concurrency limit that answer 429 with each provider's rate-limit headers.
- Token usage: Exact by construction. Ask for N tokens, receive N, and `usage` reports N. `prompt_tokens` can be set directly.
- How a test controls it: Flags and environment variables for the server, directives per request. Prometheus metrics, including a counter of injected failures. A `bench` subcommand measures TTFT, inter-token latency and tokens per second, and checks that each response delivered the requested token count.
- Architecture note: A provider-neutral engine turns the merged directives into a `Plan` (tokens, delay, tps, jitter, chunk, error fields, finish reason, seed), and thin provider adapters encode that plan as JSON or SSE.
- Sources:
  - https://github.com/enrico9034/llmock

## 8. LLMock (piyook/llm-mock)

- Identity: Real. Published on npm as `llmock`. A third project using the LLMock name.
- Language: TypeScript (Fastify). Needs Node.js 24 or later.
- License: MIT.
- Signs of life: npm `llmock` 3.16.1 published 2026-11-22. Last push 2026-11-22. 26 stars. Repo created 2024-05-22.
- How it decides what to say: Two layers. Response rules return your text, file or content blocks when the request contains a chosen string (`"in": "last"` limits the check to the last message). Everything else gets a generated reply chosen by `responseType`: `lorem` (random lorem ipsum, the default, with a `maxLoremParas` cap) or `stored` (one text picked at random from a pool, either the bundled sentences or your own JSON file).
- Sequencing: A rule with `times: 1` answers once and removes itself, so a test adds one rule per step ("draft" first, "final" second, or a 529 first and a normal reply on the retry).
- Streaming: SSE. The server waits for the configured response delay, then sends chunks at least 50 ms apart. Chunk size control is unconfirmed.
- Tool calls: Anthropic format only. A rule's `content` holds an optional text block then one or more `tool_use` blocks. Streamed, each `tool_use` block goes out as `content_block_start`, then its input JSON split into a few `input_json_delta` events, then `content_block_stop`. The follow-up round is matched on the text of the tool result with `"in": "last"`. Their docs list OpenAI and Gemini tool calls as out of scope for now.
- API shapes: OpenAI chat completions and the Responses API, Anthropic Messages, Gemini, embeddings.
- Error and chaos injection: Chaos mode fails every call or every Xth call with a chosen status, or breaks a stream part-way. A rule can `fail` one prompt, and `afterChunks` cuts a stream after N deltas (tool input deltas count).
- Record and replay: Unknown calls can go to the real provider once and be saved as fixtures.
- Token usage: Unconfirmed.
- How a test controls it: `startServer({ port: 0 })` runs it inside the test process, one server per process because settings live in environment variables and module state. An admin HTTP API changes a running server: `POST /admin/rules`, `POST /admin/reset`, `PATCH /admin/settings`, `PATCH /admin/chaos`, `GET /admin/requests`. In-process helpers `requests(filter)`, `lastRequest(filter)`, `clearRequests()`. A live dashboard and request log.
- Sources:
  - https://github.com/piyook/llm-mock
  - https://raw.githubusercontent.com/piyook/llm-mock/main/README.md
  - https://llmock.piyook.com/guide/response-types
  - https://llmock.piyook.com/guide/tool-calls
  - https://llmock.piyook.com/guide/streaming
  - https://llmock.piyook.com/guide/testing-from-code

## 9. AI-Mocks on Mokksy (Kotlin)

- Identity: Real. Mokksy is a Ktor-based mock HTTP server "inspired by WireMock" that was built because WireMock handles SSE and streaming poorly. AI-Mocks is a set of provider-specific mocks on top of it.
- Language: Kotlin, for JVM tests (Maven Central artifacts such as `ai-mocks-openai-jvm`).
- License: Apache-2.0.
- Signs of life: GitHub release v0.9.0 on 2026-06-07. Last push 2026-11-20. 53 stars. Repo created 2025-01-30.
- How it decides what to say: A request-spec DSL. `openai.completion { model = ..; temperature = ..; systemMessageContains(".."); userMessageContains("..") } responds { assistantContent = "Hello"; finishReason = "stop"; delay = 200.milliseconds }`. The block on the left says how the incoming request must look and the block on the right says what comes back.
- Streaming: `respondsStream { responseChunks = listOf("All", " we", " need"); delay = 50.milliseconds; delayBetweenChunks = 10.milliseconds }`, or a Kotlin `Flow` that emits the chunks. The test supplies the chunk boundaries.
- Tool calls: Unconfirmed. The OpenAI and Anthropic docs pages I opened show text, embeddings and moderation examples.
- API shapes: OpenAI chat completions, Responses, embeddings, moderations. Anthropic Messages. Gemini. Ollama. A2A protocol.
- Error and chaos injection: `respondsError { body = ..; httpStatus = ..; delay = ..; contentType = .. }`. Deterministic, per stub.
- Latency simulation: `delay` before the answer and `delayBetweenChunks`.
- Token usage: Unconfirmed.
- How a test controls it: In-process `MockOpenai(verbose = true)` on a random free port, `baseUrl()`, `shutdown()`. Request verification features are unconfirmed.
- Sources:
  - https://github.com/mokksy/ai-mocks
  - https://raw.githubusercontent.com/mokksy/ai-mocks/main/README.md
  - https://mokksy.dev/docs/ai-mocks/openai/
  - https://mokksy.dev/docs/ai-mocks/anthropic/

## 10. llm-d-inference-sim (vLLM simulator)

- Identity: Real. Not on the lead list, found by search. A simulator of the vLLM serving engine for testing schedulers, routers and clients with zero GPUs. It is the best example of "generate plausible output with zero model".
- Language: Go.
- License: Apache-2.0.
- Signs of life: Last push 2026-11-22. 245 stars. Repo created 2025-04-09. The README references container image tag v0.9.0.
- How it decides what to say: Two modes. Echo mode returns the last message of a chat request (or the prompt of a completion request), truncated or padded to fit token limits. Random mode (the default) picks a target length, then builds text by joining sentences from a built-in list, trimming or appending until the length is met. The target length comes from a six-bucket histogram over `max_tokens`, or from a Gaussian (mean 40, standard deviation 20) when the request sets no maximum. A `seed` setting fixes the random source. An optional SQLite dataset maps a hash of the prompt to a stored response.
- Finish reason: `length` when the response reaches the maximum, otherwise `stop`.
- Structured output: A `json_object` response format gets an object with one string field. A `json_schema` format gets a value generated from the schema.
- Tool calls: Generated from the tool schemas in the request, with random argument values inside configured ranges. Settings control integer and float ranges, array lengths, the chance of including optional parameters, and the chance of an extra tool call. `tool_choice: "required"` yields at least one call. Handling of the follow-up round is unconfirmed.
- Streaming: Time to first token and inter-token latency, each with a standard deviation, plus a factor that slows responses as concurrent load rises. Prefill time can scale with prompt length.
- API shapes: `/v1/chat/completions`, `/v1/completions`, `/v1/responses`, `/v1/embeddings`, `/v1/models`, plus vLLM's gRPC interface on the same port.
- Error and chaos injection: `failure-injection-rate` (0 to 100) and `failure-types` chosen from rate_limit, invalid_api_key, context_length, server_error, invalid_request, model_not_found.
- Token usage: It counts tokens with a real HuggingFace tokenizer for real model names, or with a fast regex tokenizer for made-up model names.
- How a test controls it: Flags, environment variables or a YAML config file, as a binary or a container. It mirrors vLLM's Prometheus metrics.
- Sources:
  - https://github.com/llm-d/llm-d-inference-sim
  - https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/README.md
  - https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/response_generation.md
  - https://github.com/llm-d/llm-d-inference-sim/blob/main/docs/configuration.md

## Smaller or narrower projects

### mockllm (StacklokLabs, and the lukehinds copy)

- Identity: Both leads are real. `StacklokLabs/mockllm` is the main repo and is archived. `lukehinds/mockllm` has the same README text.
- Language: Python.
- License: Apache-2.0.
- Signs of life: `StacklokLabs/mockllm` archived, last push 2026-05-29, 104 stars. PyPI `mockllm` 0.0.8 uploaded 2025-09-18. `lukehinds/mockllm` last push 2025-07-04, 1 star.
- How it decides what to say: A `responses.yml` file maps a prompt string to a reply string. Anything else gets the `unknown_response` default. The file hot-reloads. Whether the prompt match is exact or substring is unconfirmed.
- Streaming: Character by character.
- Latency: `lag_enabled` and `lag_factor`, applied per character with slight random variation.
- API shapes: OpenAI chat completions and Anthropic Messages.
- Token usage: The README lists "mock token counting". The formula is unconfirmed.
- Tool calls and error injection: Unconfirmed. The README shows text replies only.
- Sources:
  - https://github.com/StacklokLabs/mockllm
  - https://raw.githubusercontent.com/StacklokLabs/mockllm/main/README.md
  - https://raw.githubusercontent.com/lukehinds/mockllm/main/README.md

### mock-openai-api (zerob13)

- Identity: Real.
- Language: TypeScript (Express).
- License: MIT.
- Signs of life: GitHub release v1.0.6 on 2026-07-22. Last push 2026-11-22. 33 stars. npm `mock-openai-api` shows 1.0.3 (2025-06-13).
- How it decides what to say: Two ways. Built-in scenarios are picked by model name: `mock-gpt-thinking` shows a reasoning process, `gpt-4-mock` returns function calls, `mock-gpt-markdown` returns a fixed Markdown document, `gpt-4o-image` returns images. A built-in test case has a trigger `prompt` keyword, a `response`, optional `streamChunks`, and an optional `functionCall`. The newer path is record and replay: press Record in the admin console, send requests, press Stop, and the recording is loaded for replay.
- Replay rule: Each incoming generation call takes the next recorded response in order, with zero matching on method, path, body, model or prompt. The call after the last recording returns `recording_exhausted`. Same-protocol replays keep the original bytes, chunks, headers, status and relative timing. A response requested through a different protocol is transcoded.
- API shapes: OpenAI chat completions, Responses API, Anthropic Messages, models, image generation.
- How a test controls it: A separate admin listener (port 3001) with a Recorder, a replay playlist, a Scenario Editor (text, tool-call, usage, finish, error and ping timelines), and an API test page.
- Sources:
  - https://github.com/zerob13/mock-openai-api
  - https://raw.githubusercontent.com/zerob13/mock-openai-api/master/README.md

### mock-openai-server (freakynit)

- Identity: Real.
- Language: JavaScript (Express).
- License: MIT.
- Signs of life: Last push 2025-01-28. 13 stars.
- How it decides what to say: Sample responses per model in `config.yaml`.
- Tool calls: Each configured function has a `regexToMatchAgainstPrompt`, a `functionName` and fixed `arguments`. When tools are requested and the regex matches the prompt, that call is returned.
- Other points: A configurable random delay range. Embeddings are generated on the fly and the same input always gives the same vector. It also covers images and audio endpoints.
- Sources:
  - https://github.com/freakynit/mock-openai-server
  - https://raw.githubusercontent.com/freakynit/mock-openai-server/main/README.md

### mockai (polly3d)

- Identity: Real.
- Language: JavaScript.
- License: MIT.
- Signs of life: Last push 2026-02-05. 88 stars.
- How it decides what to say: An environment variable picks the mode: echo, random (a reply picked from a text file split on a separator), or fixed.
- Other points: Streaming and single responses. `RESPONSE_DELAY_MS` delays every response and an `x-set-response-delay-ms` request header overrides it per call.
- Sources:
  - https://github.com/polly3d/mockai
  - https://raw.githubusercontent.com/polly3d/mockai/main/README.md

### MockAI (ajac-zero/mock-ai)

- Identity: The lead `ajac-zero/MockAI` returned 404. The real repo is `ajac-zero/mock-ai`, published on PyPI as `ai-mock`.
- Language: Python.
- License: MIT per PyPI. The GitHub API license field is empty.
- Signs of life: PyPI `ai-mock` 0.3.1 uploaded 2025-03-07. Last push 2025-05-21. 14 stars.
- How it decides what to say: By default it echoes the last message. A `mock-response` request header sets the reply for that one call. A header value starting with `f:` followed by JSON makes the reply a function call. A JSON file can map an input string to a text output or a function-call output (a list gives parallel tool calls).
- Streaming: One letter per chunk.
- API shapes: OpenAI and Anthropic.
- Sources:
  - https://github.com/ajac-zero/mock-ai
  - https://raw.githubusercontent.com/ajac-zero/mock-ai/main/README.md

### FakeAI (ajcasagrande/fakeai)

- Identity: Real.
- Language: Python.
- License: Apache-2.0.
- Signs of life: PyPI `fakeai` 0.3.2 uploaded 2025-10-06. Last push 2025-10-08. 2 stars.
- What it is: A very wide simulation of the OpenAI API plus several NVIDIA services, aimed at performance testing. The README lists streaming with time to first token and inter-token latency (flags and environment variables, each with a variance percent), model-specific latency profiles, error injection by rate and type (500, 502, 503, 504, 429, context length), rate-limit tiers, function calling and structured outputs.
- How it decides what to say: Unconfirmed. An optional install extra pulls in tiktoken, transformers and torch for "LLM generation", so the default generator is something lighter that I did not pin down.
- Sources:
  - https://github.com/ajcasagrande/fakeai
  - https://pypi.org/project/fakeai/

### openai-responses-python (mharrisb1)

- Identity: Real, but it is a pytest plugin, not a server. It intercepts HTTPX calls in-process with RESPX.
- Language: Python.
- License: MIT.
- Signs of life: PyPI `openai-responses` 0.13.1 uploaded 2025-12-02. Last push 2025-12-02. 55 stars. The README says the project is in maintenance mode and may be archived.
- Coverage: Chat, embeddings, files, models, moderations, assistants, threads, messages, runs, vector stores. Streaming responses were added in v0.4.
- Sources:
  - https://github.com/mharrisb1/openai-responses-python
  - https://raw.githubusercontent.com/mharrisb1/openai-responses-python/main/README.md

### WireMock MockGPT and the WireMock Cloud OpenAI template (commercial, hosted)

- Identity: Real. MockGPT is a free hosted mock at `https://mockgpt.wiremockapi.cloud/v1`, powered by WireMock Cloud.
- How it decides what to say: A default stubbed response. A text box on the MockGPT page sets the next response. In your own WireMock Cloud copy you can add responses keyed to prompts, model or tuning parameters, simulate faults on demand, add delays, and build stateful sequences with the Scenarios feature. The free account allows 1000 API calls a month.
- The OpenAI template: 237 endpoints and 421 stubs generated from OpenAI's OpenAPI description (spec version 2.3.0). The content string is fixed. With `stream: true` the template returns the complete chat completion JSON in one response; their FAQ says to stub a server-sent-events body yourself in a private copy if you need real event-by-event delivery. The shared sandbox serves success responses only.
- Fit for us: A hosted service means network calls, which rules it out for fast local tests. The Scenarios idea (a named state machine) is the same one llmposter uses.
- Sources:
  - https://mockgpt.wiremock.io/
  - https://www.wiremock.io/api-mocks/openai

### llm-mock (Rust crate by MukundaKatta)

- Identity: Real, tiny. It is an in-process object, not an HTTP server.
- Language: Rust, with `serde_json` as its only dependency.
- License: MIT.
- Signs of life: crates.io 0.1.0 published 2026-05-25, 25 downloads. Last push 2026-06-13. 0 stars.
- How it works: `mock.when_contains("hello", MockResponse::text("hi there"))`, `when_exact`, `always`, `set_default`. Rules are checked in registration order and the first match wins, against the serialized messages. `MockResponse::tool_call(name, input)` gives a tool-use reply. `call_count()`, `last_call()`, `call_log()`, `reset_log()` record what was sent. The reply JSON has `content`, `tool_calls`, `input_tokens`, `output_tokens`, `stop_reason`.
- Sources:
  - https://raw.githubusercontent.com/MukundaKatta/llm-mock-rs/main/README.md
  - https://crates.io/crates/llm-mock

### MSW-based mocks

- What I confirmed: Only aimock's migration page, which is a competitor's description. It shows the MSW pattern as a hand-written `ReadableStream` that enqueues each `data: {...}` SSE line yourself, about 35 lines for one streamed reply. It also points out that MSW intercepts inside one process, so a second process (a gateway, a worker) never sees the mock.
- A dedicated, maintained MSW package for OpenAI: unconfirmed.
- Sources:
  - https://aimock.copilotkit.dev/migrate-from-msw

## Leads that changed, stayed unconfirmed, or were only glimpsed

- `CopilotKit/llmock`: Real, renamed. The GitHub API resolves it to `CopilotKit/aimock`.
- `ajac-zero/MockAI`: That exact URL is a 404. The project lives at `ajac-zero/mock-ai`.
- `lukehinds/mockllm`: Real, with the same README as `StacklokLabs/mockllm`, which is archived.
- `openai-mock`: Unconfirmed as a Python or Node package. PyPI and npm both returned 404 for that name. crates.io has a Rust crate named `openai-mock` at 0.1.0 (updated 2024-11-25, "A Rust library to mock OpenAI's API for testing purposes"), and the repository it lists (`socrates8300/openai-mock`) returned 404 from the GitHub API, so I could not read how it works.
- `openai-api-mock` (npm, by chihebnabil): Exists. The npm registry shows 0.4.1 published 2026-08-30, MIT, described as a Node module that mocks OpenAI-compatible responses (chat, embeddings, images). The GitHub API shows JavaScript, 27 stars, last push 2026-11-09. I could not open its README, so its mechanics are unconfirmed.
- Mockoon OpenAI templates: Unconfirmed. The sample URL I tried returned 404 and the Mockoon pages that came up describe generic templates and an AI assistant that generates JSON templates from a prompt. Mockoon can run any OpenAPI file as a mock, which is the likely route.
- `fakeai`: Real (see the FakeAI section). Its reply generator is unconfirmed.
- `llm-mock-server`: Real (theblixguy, section 6). A second project with a similar name, `higress-group/mock-server` (Go, 16 stars, last push 2026-07-10), describes itself as an LLM mock server for Higress end-to-end tests. I read its GitHub metadata only.
- `pytest-mockllm` and `autopost/llm-mock`: Both came up in search results as pytest plugins that patch the OpenAI and Anthropic SDKs in-process. I did not open their pages, so treat them as unverified names.
- `wiremock` (Rust crate): Exists on crates.io at 0.6.5 (updated 2025-08-24) as a general HTTP mocking crate. A search result showed a Rust project's test file whose comment says wiremock serves a whole SSE fixture as one body, which tests byte-level parsing but not slow chunk delivery. I did not open that file, so this is a lead, not a finding.
- LiteLLM's built-in mock response feature: Not checked.
- Rust crates on crates.io that I confirmed for this job: `vidaimock`, `llmposter`, `openai-mock-server`, `llm-mock`. Names I probed that returned 404: `mock-openai`, `mockllm`, `fake-llm`, `llm-test-double`.

## Patterns across projects

### How the reply gets chosen

There are three families, and the strongest tools mix them.

- Scripted or matched fixtures: aimock, llmposter, llm-mock-server, mock-llm, AI-Mocks, piyook's LLMock rules, openai-mock-server. Almost all of them match on the last user message, by substring first and regex second. Whole-conversation matching is rare, and aimock says outright that earlier turns are invisible unless you write a predicate.
- Generated text with zero model: echo of the last message (mock-llm default, MockAI, polly3d mockai, llm-d-inference-sim echo mode), lorem ipsum at a requested length (enrico9034 LLMock, piyook LLMock default), random sentences from a built-in list (llm-d-inference-sim), a random pick from a pool (piyook `stored`, polly3d `random`), a template rendered from the request (VidaiMock, mock-llm, llmposter `content_template`).
- Recorded tapes: aimock, llmposter, zerob13 and piyook can all proxy the real API once and replay later. zerob13 replays strictly in order with zero matching. The others turn recordings into matchable fixtures.

Ordering rules differ, and each project had to document its choice because people trip on it: first match wins (aimock, llmposter, llm-mock-server, `llm-mock` crate), last match wins (mock-llm), priority numbers plus a separate catch-all pass (llmposter). aimock and llmposter both lint fixtures at load time to catch a broad rule that shadows a narrow one.

What happens on a miss also differs: 404 (aimock, llmposter), 501 (openai-mock-server), 503 in strict mode (aimock), a default fallback reply (llm-mock-server, mockllm), echo (mock-llm), or generated filler (piyook, enrico9034).

### Multi-turn and tool loops

The same user message arrives twice in a tool round trip: once before the tool runs and once after, with the tool result appended. Every project needed an answer to that. I saw five:

- A counter: `sequenceIndex` (aimock), `sequence` (mock-llm), a reply queue (openai-mock-server), `replySequence` and `.times(n)` (llm-mock-server), `times: 1` rules (piyook).
- A check derived from the request alone: "is there a tool result after the last user message" (aimock `hasToolResult`, VidaiMock `has_tool_result`), "how many assistant messages are in the history" (aimock `turnIndex`), "what is the last message" (piyook `in: last`).
- The tool call id: aimock `toolCallId`, llm-mock-server `whenToolResult`. This needs the fake to emit a stable, known id.
- The tool result's content: aimock `toolResultContains`, piyook matching on result text.
- A named state machine: llmposter scenarios, WireMock Scenarios.

aimock's docs argue for the request-derived checks because counters drift when tests run in parallel against one server. VidaiMock goes furthest: its default behavior with zero setup is "tools offered and no tool result yet means call the first tool, tool result present means answer in text", which lets an agent loop end naturally.

Triggering a tool call is nearly always explicit (the fixture says which tool and which arguments). The exceptions are VidaiMock (echoes the first offered tool with empty arguments), freakynit (a regex on the prompt), and llm-d-inference-sim (random arguments generated from the tool's JSON schema).

### One neutral reply, many wire formats

aimock, llmposter, llm-mock-server and enrico9034's LLMock all separate "what to say" from "how it looks on the wire". A fixture or plan says text, tool calls, finish reason, usage and timing. A per-provider encoder turns that into chat-completions JSON, chat-completions SSE, Responses events, or Anthropic events. Finish reasons are mapped per provider (aimock documents the mapping table). This is why one fixture can serve every API shape, and why adding the Responses API or Anthropic later is a new encoder and not a new mock.

### Streaming

- Splitting: by characters with a `chunkSize` (aimock, llmposter, llm-mock-server, mock-llm with a default of 50), by words (VidaiMock), by tokens with a tokens-per-frame setting (enrico9034), one character per chunk (mockllm, MockAI), or exactly the pieces the test supplies (openai-mock-server, AI-Mocks).
- Pacing: a fixed gap between chunks is the baseline everywhere. The richer model is time to first token, then tokens per second, then jitter (aimock, enrico9034, VidaiMock, llm-d-inference-sim, FakeAI).
- Tool-call arguments: usually sent in one frame (llmposter and VidaiMock both say so and explain why). piyook splits Anthropic tool input into a few deltas. aimock has an explicit fault for a stream that stops in the middle of tool arguments.
- The final usage chunk for `stream_options.include_usage` is handled by aimock, VidaiMock and enrico9034.

### Failure injection comes in three layers

- HTTP-level: a status code with the provider's error envelope, often with `Retry-After` for 429. A one-shot "fail the next request" call shows up in aimock (`nextRequestError`), llm-mock-server (`nextError`) and openai-mock-server (a queued `Reply::error`).
- Stream-level: cut after N chunks (aimock, llmposter, piyook), disconnect after N milliseconds (aimock, llmposter), an error event in the middle of a stream (openai-mock-server `error_after`, enrico9034 `error_after`), a stream that ends with `[DONE]` missing (openai-mock-server), a hang before headers or mid-stream (enrico9034), duplicated frames (llmposter), keep-alive comment lines (openai-mock-server), a network write that ends in the middle of an SSE event (VidaiMock `:vidai-split`), a broken JSON body (aimock, VidaiMock, llmposter).
- Model-level: the HTTP exchange is healthy but the model "misbehaves": invalid JSON in tool arguments, an unknown tool name, duplicate tool call ids, a length stop in the middle of a tool call, an empty reply, a refusal. aimock is the only project I found with a catalog for this, and llmposter has refusals.

Probabilistic chaos is common (aimock, VidaiMock, FakeAI, llm-d-inference-sim, piyook's every-Xth call). Reproducible chaos is rare: llmposter seeds it, enrico9034 has a `seed` directive, aimock seeds its misbehavior rates.

### How tests steer the mock

- In-process server on port 0, one per test, stopped on drop or scope exit: llmposter, openai-mock-server, VidaiMock, llm-mock-server, aimock, AI-Mocks, piyook. This is the standard shape for per-test isolation.
- Per-request headers: VidaiMock (`X-Mock-Status`, `X-Vidai-Chaos-*`, `X-Vidai-Usage`, `X-Vidai-Output-Tokens`, `X-Vidai-Scenario`), aimock (`x-aimock-chaos-*`, `X-Test-Id`, `X-AIMock-Context`), MockAI (`mock-response`), polly3d (delay header).
- Directives inside the prompt: enrico9034's `[[mock ...]]` block. This is the only control channel that survives a gateway that rewrites or drops headers.
- Admin HTTP API on a running server: aimock (`/__aimock/...`), mock-llm (`/config`), piyook (`/admin/...`), zerob13 (a separate admin port).
- Shared-server isolation: aimock scopes chaos and recordings by an `X-Test-Id` header.
- Request recording for assertions: aimock journal, llmposter `get_requests()` with an outcome per request, openai-mock-server typed `requests::<T>()` plus `rejected()`, llm-mock-server `history`, piyook `requests()`. llm-mock-server also exposes "were all my limited rules used up" (`isDone()`).

### Token usage

- Fixed constants: openai-mock-server (9, 12, 21), mock-llm samples (10, 20, 30).
- A cheap estimate from length: aimock (content length, about 4 characters per token), llmposter (bytes divided by 4).
- Exact by construction: enrico9034 generates exactly N tokens and reports N. VidaiMock's `X-Vidai-Output-Tokens` header does the same with words.
- Pinned per request: VidaiMock's `X-Vidai-Usage` header, including cached and reasoning token counts.
- A real tokenizer: llm-d-inference-sim for real model names.
- Overridable per fixture: aimock.
- Replayed from a recording: aimock.

### Determinism details that are easy to miss

- Ids and timestamps: llmposter uses sequential ids and openai-mock-server uses fixed ids and a fixed `created` time, which makes snapshot tests stable. VidaiMock's templates use random UUIDs and the current time.
- SDK retries: llm-mock-server noticed that official SDKs retry on their own and replays the previous reply for a retried request, so a retry does not eat a step of the script.
- Spec validation: openai-mock-server decodes both the request and the scripted reply against models generated from OpenAI's spec, so a malformed request from the client or a malformed fixture fails the test. VidaiMock and llmposter validate a few required fields by hand.

## Ideas worth copying for a Rust in-process fake

- Split the fake into a neutral reply plan and a wire encoder. The rule engine returns a plan (text or tool calls, finish reason, usage, chunking, timing, faults) and a separate encoder writes chat-completions JSON or SSE. Seen in enrico9034's LLMock, llmposter, llm-mock-server and aimock. Confidence: high - four independent projects converged on it and it keeps a later Responses or Anthropic encoder cheap.
- Make the default tool-loop behavior stateless. With zero scripting: tools offered and no tool result after the last user message means emit a tool call; a tool result present means answer in text. Then add match fields derived from the request (has tool result, assistant turn count, last tool call id) and keep an ordered script as the fallback for exact sequences. Seen in VidaiMock and aimock. Confidence: high - it removes shared counters, so parallel tests cannot interfere.
- Let the prompt steer the fake. A directive block such as `[[mock tokens=50 tps=10 error=429]]` in the last user message travels through the harness and the gateway with zero side channel, and the fake can echo the plan it used in a response header. Seen in enrico9034's LLMock. Confidence: medium - one young project uses it, but it fits a gateway under test unusually well; the directive text must be stripped or tolerated wherever prompts are asserted.
- Treat stream faults as first-class, seeded options: gap between events, keep-alive comments, error event after N events, cut after N chunks, disconnect after N milliseconds, missing `[DONE]`, duplicated frame, malformed chunk, a write split in the middle of an event, hang. Add the model-level faults (invalid tool-argument JSON, unknown tool, length stop mid tool call) as a second list. Seen in openai-mock-server, llmposter, aimock and VidaiMock. Confidence: high - these are the failure paths a gateway and a harness actually have to survive, and a seed keeps them repeatable.
- Record every request with an outcome and fail loudly on a miss. Keep a journal (matched rule, outcome, interrupted or not), return a clear error for an unmatched request in strict mode, use sequential ids and a fixed clock, and compute usage by a simple exact rule so tests can assert numbers. Seen in llmposter, openai-mock-server, aimock and enrico9034's LLMock. Confidence: high - cheap to build and it is what makes failures debuggable.
- Reuse or build: llmposter's license is AGPL-3.0, so depending on it or borrowing from it needs a license decision first. openai-mock-server (MIT OR Apache-2.0) is four days old and scripts by order only. VidaiMock (Apache-2.0) is usable in-process today; its replies are configured through template and scenario files plus request headers, and its Rust API covers start, address and shutdown, which leaves request capture to the test. Building our own small fake and borrowing ideas looks like the better path. Confidence: medium - based on docs only, none of these were run.

