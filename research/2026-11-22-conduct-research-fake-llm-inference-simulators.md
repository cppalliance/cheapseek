---
produced: 2026-11-22
title: Fake LLM inference server simulators and mock backends for testing gateways, proxies, routers and load tools - llm-d-inference-sim, TensorZero dummy provider, LiteLLM mock_response, GuideLLM and AIPerf mock servers, Dynamo mocker, vLLM mock engine, Envoy AI Gateway testupstream
---

# Fake LLM backends: inference-server simulators and gateway mock upstreams

This is a survey of what people have built to stand in for a real model server when they test gateways, routers, autoscalers and benchmark tools. The goal behind it: a fast, local, deterministic fake backend for a Rust agent harness and gateway that speak the OpenAI chat-completions API (JSON and SSE, with tool calls).

How to read this file:

- Every fact below comes from a page I fetched on 2026-11-22: a README, a docs page, or a source file. The source URLs sit at the end of each section.
- Where I only saw a search-result excerpt of a page, or only a directory listing, the bullet says so.
- CLI option names are written without their leading dashes, so `mode echo` means the `mode` flag set to `echo`.
- Stars, license and last-push dates come from the GitHub API on 2026-11-22 where given.

## 1. llm-d-inference-sim (vLLM simulator)

The most complete inference-server simulator I found. It is a standalone HTTP server that pretends to be vLLM (and, experimentally, SGLang).

- Language: Go
- License: Apache-2.0
- Signs of life: 245 stars, 199 forks, last push 2026-11-22. The README pins image tag v0.9.0, and a manifest in llm-d/llm-d-router uses v0.10.2 (seen in a search excerpt).
- Endpoints: `/v1/chat/completions`, `/v1/completions`, `/v1/responses`, `/v1/embeddings`, `/v1/models`, Anthropic-style `/v1/messages`, `/tokenize`, vLLM render and derender endpoints, `/metrics`, `/health`, `/health/ready`, LoRA load and unload, plus a gRPC interface on the same port.

How it picks the response text:

- `mode echo`: returns the content of the last message (chat) or the prompt (completions).
- `mode random` (the default): picks a random sentence from a built-in list, trims it if it is too long, and appends more sentences until the target token length is met.
- Optional dataset: a SQLite file (local path or a URL, including HuggingFace) keyed by a hash of the tokenized prompt. A hit returns the stored response. A miss falls back to a random dataset response or the built-in sentences.

How it decides length, `usage` and `finish_reason`:

- With `max_tokens` or `max_completion_tokens` set, the length is sampled from a six-bucket histogram with probabilities 20%, 30%, 20%, 5%, 10%, 15%. For limits of 120 tokens or less the buckets are equal in size. Above 120, buckets 1, 2, 3, 5 and 6 are 20 tokens wide and bucket 4 covers the rest.
- With the limit unset, the cap is the context window minus the input length, and the length is drawn from a Gaussian with mean 40 and standard deviation 20. If that cap is under 20 the draw is uniform.
- `finish_reason` is `length` when the response reaches the maximum, otherwise `stop`. Other values it can return: `tool_calls`, `remote_decode`, `cache_threshold`.
- `ignore_eos: true` forces the response to exactly `max_tokens` by padding. `min_tokens` pads up to a floor.
- Context window: default 1024 tokens (`max-model-len`). A request whose input fills the window gets a 400.
- Token counting: with a made-up model name it uses an in-process regex tokenizer (word runs, each other character on its own, trailing whitespace attached to the previous token) and FNV-32a hashes as token ids. With `render-url` set it forwards tokenization to a real vLLM render service for exact counts.
- `usage` includes `prompt_tokens`, `completion_tokens`, `total_tokens` and `prompt_tokens_details.cached_tokens`.

Latency model and knobs (all default to zero, so out of the box it answers instantly):

- `time-to-first-token` plus `time-to-first-token-std-dev`.
- `inter-token-latency` plus `inter-token-latency-std-dev`. Decode time is `(output_tokens - 1) x inter-token-latency`.
- `latency-calculator`: `constant` (flat TTFT) or `per-token`, where prefill is `prefill-overhead + (n - n_cached) x prefill-time-per-token`. `n_cached` is the number of prefix-cache hits, so a warm cache shortens TTFT.
- Jitter is Gaussian. Each std-dev is capped at 30% of its mean and samples are clamped to within 70% of the mean. Durations are rounded to whole milliseconds, so a std-dev under 1 ms has no effect.
- `time-factor-under-load` (1.0 or more): multiplies the GPU-bound latencies, scaling linearly from 1.0 with one request in flight up to the configured factor at the concurrency limit.
- Concurrency limit `max-num-seqs` defaults to 5, waiting queue defaults to 1000.
- `startup-duration`: `/health/ready` returns 503 for that long to imitate model load.
- KV-cache transfer latencies for prefill/decode disaggregation, constant or per prompt token.
- The docs ship reference tables (TTFT and ITL by model size and GPU) and three ready YAML profiles, for example 8B on H100: TTFT 100 ms, ITL 12 ms, load factor 2.0.
- All latency settings can be changed on a running simulator through `POST /admin/config`.

Streaming:

- Accepts `stream` and `stream_options.include_usage` on chat and completions.
- `stream-interval` sets how many tokens go in each streamed chunk. The first token is always sent alone.
- The exact chat chunk sequence (role chunk, usage chunk, `[DONE]`) is something I did not confirm from the docs. The docs spell out the full event order only for `/v1/responses`.

Tool calls and structured output:

- When the request has `tools`, it synthesizes arguments from each tool's JSON schema using random values. Knobs set integer and number ranges (default 0 to 100), array lengths (default 1 to 5), and the probability of including optional parameters and optional object fields (default 50).
- `tool_choice: "required"` yields at least one call, `"auto"` yields zero or more. `tool-call-extra-call-probability` (default 45) drives extra calls, a truncated geometric distribution.
- Incoming tool schemas are validated against a strict subset of JSON Schema and rejected with 400 unless `skip-tool-validation` is on.
- `response_format` of type `json_object` returns an object with one string field named `text`. Type `json_schema` returns a value generated from the schema. If `max_tokens` cuts it short the JSON is left incomplete with `finish_reason` `length`. Echo mode ignores `response_format`.

Failure injection:

- `failure-injection-rate` (0 to 100) and `failure-types` chosen from `rate_limit`, `invalid_api_key`, `context_length`, `server_error`, `invalid_request`, `model_not_found`.
- Request header `X-Return-Error: <status>` makes that one request fail with the given HTTP status, bypassing the random rate. This is the deterministic route.
- Both can be changed at runtime via `POST /admin/config`.
- Every failure setting I read describes an error response returned in place of the completion. Cutting a stream midway is unconfirmed for this tool.

Repeatability:

- `seed` sets the random seed. Left unset, it uses the current Unix time in nanoseconds.

Other things it fakes:

- Prometheus metrics in vLLM's names, either real counts from the simulated traffic or `fake-metrics` values, including generator functions (`oscillate`, `ramp`, `rampreset`, `squarewave`) in the form `fun:start:end:period`.
- LoRA adapters: load and unload endpoints plus LoRA metrics.
- KV cache: simulated block usage with ZMQ events on allocate and evict.
- Response headers naming the pod, namespace and port (when `POD_NAME` and `POD_NAMESPACE` are set), handy for checking which replica a router picked.
- `strict` mode validates requests against embedded vLLM 0.21.0 schemas.

Sources:

- https://github.com/llm-d/llm-d-inference-sim
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/README.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/configuration.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/response_generation.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/latency-simulation.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/latency-profiles.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/tokenization.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/api.md
- https://raw.githubusercontent.com/llm-d/llm-d-inference-sim/main/docs/http-endpoints.md

## 2. TensorZero dummy provider (Rust)

TensorZero's gateway has a built-in provider type `dummy`. It lives inside the gateway process, so tests exercise everything above the HTTP client. The behaviour is chosen entirely by the configured model name. I read the whole source file.

- Language: Rust
- License: Apache-2.0
- Signs of life: 11,696 stars. The GitHub API reports the repository as archived, with the last push on 2026-06-11.
- File: `crates/tensorzero-core/src/providers/dummy.rs` (about 1,090 lines).

Baseline behaviour for any name not listed below:

- Text: one fixed sentence, "Megumin gleefully chanted her spell, unleashing a thunderous explosion that lit up the sky and left a massive crater in its wake."
- Usage: `input_tokens` is always 10. `output_tokens` is the number of content blocks (non-streaming) or the number of stream chunks (streaming). It is a count of pieces, not of words.
- `finish_reason`: `ToolCall` when the model name contains `tool`, otherwise `Stop`. The request's `max_tokens` is ignored, so the output is the same fixed text whatever the limit.
- Latency: every call sleeps 1 ms ("so they don't seem like 0ms inferences"). The reported provider latency is a fixed 100 ms for non-streaming calls.
- Streaming: 16 word-sized chunks ("Wally," " the" " golden" " retriever," and so on), a 10 ms sleep after each chunk, then a final empty chunk that holds usage and the finish reason. Reported per-chunk latency is `50 + 10 x (i + 1)` ms.

Special model names, errors and delays:

- `slow`: sleeps 5 seconds before answering (both modes). For embeddings, any name containing `slow` sleeps 30 seconds.
- `slow_second_chunk`: streaming only, sleeps 2 seconds before the second chunk.
- `flaky_<anything>`: a per-name call counter in a process-wide map. Odd calls succeed, even calls fail. If the name also contains `rate_limit` the failure is a rate-limit error (non-streaming). If it contains `raw_response` the error includes a raw response body.
- `error_with_raw_response`: fails with status 500 and attached raw request and response text.
- Any name starting with `error`: fails with "Error sending request to Dummy provider".
- `err_in_stream`: streaming, yields an ordinary error at chunk index 3 and then keeps streaming.
- `fatal_stream_error`: streaming, yields a fatal stream error at chunk index 2 and then sleeps 5 seconds.
- `err_in_stream_with_raw_response` and `fatal_stream_error_with_raw_response`: the same two with a raw response body attached.
- `test_key`: fails with "Invalid API key" unless the dynamic credential equals `good_key`.
- `multiple-text-blocks`: fails unless the first message has exactly two text blocks.
- `require_pdf`: fails unless a PDF file is in the messages.
- `llm_judge::error`: fails with "Dummy error in inference".

Special model names, tool calls:

- `tool`: a call to `get_temperature` with arguments `{"location":"Brooklyn","units":"celsius"}`. Streaming sends the arguments in 5 fragments with the tool name on the first chunk only.
- `tool_split_name`: streaming, the tool name itself arrives split across the first two chunks (`get_temp` then `erature`).
- `good_tool`: a call to `output_schema_tool` with `{"sentiment":"positive","confidence":0.95}`.
- `bad_tool`: same as `tool` but with `"units":"Celsius"`, which fails enum validation.
- `invalid_tool_arguments`: a call whose arguments are the string `Not valid 'JSON'`.

Special model names, JSON and reasoning:

- `json`: `{"answer":"Hello"}` (streamed as 5 fragments of a different object, `{"name":"John","age":30}`).
- `json_goodbye`, `json_diff_schema`, `json_cot`, `json_beatles_1`, `json_beatles_2`: other fixed JSON bodies, one of them deliberately in a different schema.
- `best_of_n_0`, `best_of_n_1`, `best_of_n_big` (answer choice 100, out of range), `flaky_best_of_n_judge`: fixed judge answers for best-of-N tests.
- `llm_judge::true`, `llm_judge::false`, `llm_judge::zero`, `llm_judge::one`: fixed `{"score": ...}` bodies.
- `reasoner`: a thought block "hmmm" followed by the default text. `reasoner_with_signature` adds a signature. `json_reasoner` pairs a thought with JSON. In streaming, two thought chunks precede the content.

Special model names, echo and inspection:

- `echo`: returns the text of the last user message.
- `echo_request_messages`: returns the system prompt and all messages as JSON.
- `echo_extra_info` and `echo_injected_data`: return the extra body and extra headers the gateway would send, as JSON.
- `extract_images`: returns the file blocks found in the messages as JSON.
- `empty`: an empty string. `null`: an empty content list.
- `alternate`: a second fixed sentence.
- `random_answer`: `{"answer": "<fresh UUID>"}`, the one deliberately different-every-time output.

Special model names, usage:

- `input_tokens_zero`, `output_tokens_zero`, `input_tokens_output_tokens_zero`, `input_five_output_six`, `cache_read` (reports 7 cached input tokens).

Repeatability:

- Everything is fixed text, so runs repeat exactly apart from response ids (UUID v7), `random_answer`, and the `flaky_` counters, which are shared across the whole process and so depend on test order.

Two helpers in the same repository:

- `crates/tensorzero-core/tests/mock-provider-api`: a small axum HTTP server (default bind `0.0.0.0:3030`, optional `delay_ms` added to every request). Its chat handler returns one of three fixture files chosen by whether the request has `tools` or `response_format.type == "json_object"`. Streaming replays a JSONL fixture line by line and then sends `[DONE]`. It also fakes embeddings (1536 zeros), batch and fine-tuning endpoints.
- `crates/provider-proxy`: a caching man-in-the-middle proxy (default port 3003, cache directory `./request_cache`) that records real provider responses once and replays them in end-to-end tests, switched on with `TENSORZERO_E2E_PROXY`.

Sources:

- https://raw.githubusercontent.com/tensorzero/tensorzero/main/crates/tensorzero-core/src/providers/dummy.rs
- https://raw.githubusercontent.com/tensorzero/tensorzero/main/crates/tensorzero-core/tests/mock-provider-api/src/main.rs
- https://raw.githubusercontent.com/tensorzero/tensorzero/main/crates/provider-proxy/README.md
- https://github.com/tensorzero/tensorzero

## 3. LiteLLM: mock parameters and the fake OpenAI endpoint

LiteLLM has two separate things: request parameters that short-circuit the provider call inside LiteLLM itself, and a standalone fake OpenAI server used for proxy load tests.

- Language: Python (the GitHub description now reads "Rust core with Python SDK")
- License: GitHub reports it as "Other"
- Signs of life: 63,694 stars, last push 2026-11-22

The mock parameters (read from `litellm/main.py`):

- `mock_response`: a string that becomes the assistant content. The default text is "This is a mock request". It can also be a dict or a full `ModelResponse`.
- Magic strings in `mock_response` raise errors: `"litellm.RateLimitError"`, `"litellm.ContextWindowExceededError"` and `"litellm.InternalServerError"` raise those exception types. A string starting with `Exception: content_filter_policy` raises a 400 as if from Azure. A string starting with `Exception: mock_streaming_error` becomes an error with status 529 and the message "This is a mock error raised mid-stream". Passing an exception object raises it with its own status code (500 if it has none).
- `mock_tool_calls`: a list of tool-call dicts placed on the first choice's message.
- `mock_timeout`: sleeps for the request's `timeout` and then raises `litellm.Timeout`.
- `mock_delay`: sleeps that many seconds before answering. It only applies when one of the other mock parameters is also present. Sent alone it gets forwarded to the real provider.
- `n`: produces that many identical choices.
- Usage: fixed numbers, 10 prompt tokens and 20 completion tokens, from constants `DEFAULT_MOCK_RESPONSE_PROMPT_TOKEN_COUNT` and `DEFAULT_MOCK_RESPONSE_COMPLETION_TOKEN_COUNT` (both overridable by environment variable). The docs page still shows an older example with null usage.
- Streaming: the mock text is wrapped in LiteLLM's normal stream wrapper. The docs example shows a first chunk with role `assistant` and a three-character content piece.
- `finish_reason`: the docs example shows `stop`. The mock ignores `max_tokens`.
- Repeatability: output is exactly what the caller passed in, so it repeats.

How the proxy guards these (from `litellm/proxy/route_llm_request.py` and PR 35423, merged 2026-08-01):

- `mock_response` and `mock_tool_calls` from a client need a per-key or per-team opt-in named `allow_client_mock_response`.
- `mock_timeout`, `mock_delay`, `mock_testing_fallbacks`, `mock_testing_context_fallbacks`, `mock_testing_content_policy_fallbacks` and `mock_testing_rate_limit_error` are rejected with a 400 unless the config file sets `general_settings.dangerously_allow_mock_testing_request_params: true`.
- The PR explains why it rejects instead of silently dropping: a dropped parameter used to produce a normal 200 from the real model, which looked like a passing fallback test when nothing had been mocked.

How LiteLLM's own tests avoid real providers (partial view):

- A code search hit shows unit tests using `respx` to stub `https://api.openai.com/v1/completions`.
- The repository has `tests/e2e/perf/setup/locustfile.py` and related files that reference `mock_delay` (seen as file names in a code search, contents unread).

The fake OpenAI endpoint (`BerriAI/example_openai_endpoint`, read from `main.py`):

- Language: Python (FastAPI). License: GitHub lists no license. 14 stars, last push 2026-09-26.
- LiteLLM's load-test docs point the proxy at a hosted copy (`exampleopenaiendpoint-production.up.railway.app`) under the model name `fake-openai-endpoint`, and say you can self-host this repository instead.
- Text: one hard-coded reply, "Hello there, how may I assist you today?", with hard-coded usage of 9 prompt, 12 completion, 21 total tokens and `finish_reason` `stop`.
- Streaming: the sentence "Hello this is a test response from a fixed OpenAI endpoint." sent one word per chunk with a fixed `created` timestamp. The stream ends when the connection closes after the last word.
- Path quirk: the handler is tuned to LiteLLM's own routing. Requests to `/chat/completions` get the OpenAI shape above (and streaming). Requests to `/v1/chat/completions` get an Anthropic Messages shaped JSON body, because LiteLLM routes Vertex-hosted Claude there. Worth knowing before pointing a different gateway at it.
- Latency: environment variable `TIME_TO_SLEEP` adds a fixed sleep to every request.
- Behaviour by model name: `429` returns HTTP 429. `random_sleep` sleeps 1 to 10 seconds. `degraded`, `slow_provider` or `blocked` sleep on a three-part distribution: 70% around 10 s (Gaussian, sd 2), 25% around 60 s (Gaussian, sd 5), 5% uniform between 60 and 300 s.
- It also fakes Anthropic, Bedrock, Vertex and Gemini shapes, embeddings, audio, batch and files. One deliberately bad Gemini route fails every 200th request and a random 0.5% of the rest.
- Repeatability: ids are random UUIDs and the sleeps use Python's unseeded `random`, so timing varies run to run while the text stays fixed.

Sources:

- https://docs.litellm.ai/docs/completion/mock_requests
- https://github.com/BerriAI/litellm/blob/main/litellm/main.py
- https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/proxy/route_llm_request.py
- https://github.com/BerriAI/litellm/pull/35423
- https://docs.litellm.ai/docs/proxy/timeout
- https://docs.litellm.ai/docs/load_test
- https://raw.githubusercontent.com/BerriAI/example_openai_endpoint/main/main.py

## 4. GuideLLM mock server

GuideLLM is a benchmarking tool. It ships its own mock OpenAI and vLLM server so the benchmark code can be tested with zero GPUs.

- Language: Python (Sanic)
- License: Apache-2.0
- Signs of life: 1,802 stars, last push 2026-11-22
- Location: `src/guidellm/mock_server`, configured by `MockServerConfig` with environment prefix `GUIDELLM_MOCK_SERVER_`.
- Endpoints: `/v1/chat/completions`, `/v1/completions`, `/v1/responses`, `/tokenize`, `/detokenize`, `/v1/models`, `/health`, audio transcription and translation.

How it works:

- Text: filler prose from the Faker library, seeded with a fixed seed of 42, generated until it holds exactly the wanted number of tokens. The same token count therefore always gives the same text.
- Length: `min(sample(output_tokens, output_tokens_std), max_tokens)`. `output_tokens` defaults to 128 with a std-dev of 0.
- Tokenizer: a mock regex tokenizer, `\w+|[^\w\s]|\s+`, so words, punctuation marks and whitespace runs each count as a token. A real HuggingFace tokenizer can be plugged in with the `processor` setting.
- Usage: prompt tokens come from tokenizing the concatenated message contents. Images, video and audio add flat charges (576 per image, 1024 per video, 25 per second of audio).
- `finish_reason`: `stop` on every text response, including ones capped by `max_tokens`. `tool_calls` on tool turns.
- Latency: `ttft_ms` (default 150) and `itl_ms` (default 10), each with a std-dev setting (default 0), sampled from a Gaussian and floored at zero. Non-streaming requests sleep for TTFT plus the sum of the inter-token gaps before replying.
- Streaming: one content delta per token, then a final chunk with an empty delta and the finish reason, then (only when `stream_options.include_usage` is set) a chunk with empty `choices` and the usage block, then `data: [DONE]`. Each content delta holds only a `content` field, so the stream opens straight into text with the `role` field left out.
- Tool calls: only when `tool_choice` is `"required"` and tools are present. It calls the first tool by name with arguments `{}`. Streaming sends an opening tool-call delta with id, type and name, then argument fragments one character at a time, then a `tool_calls` finish chunk.
- Failure and load knobs: `fail_after_requests` makes every generation request after the Nth return HTTP 500. `max_concurrent_requests` is a semaphore, extra requests wait for a slot, and a streaming request holds its slot until the stream finishes or the client disconnects.
- Bad input gets a 400 with an OpenAI-style error body.
- Repeatability: text repeats (fixed Faker seed). Timing jitter uses Python's global `random.gauss`, and the files I read leave it unseeded, so keep the std-dev settings at 0 for repeatable timing.

Sources:

- https://raw.githubusercontent.com/vllm-project/guidellm/main/src/guidellm/mock_server/server.py
- https://raw.githubusercontent.com/vllm-project/guidellm/main/src/guidellm/mock_server/utils.py
- https://raw.githubusercontent.com/vllm-project/guidellm/main/src/guidellm/mock_server/handlers/chat_completions.py
- https://raw.githubusercontent.com/vllm-project/guidellm/main/src/guidellm/schemas/mock_server/config.py

## 5. AIPerf mock server (NVIDIA)

AIPerf is NVIDIA's benchmark tool (the successor line to GenAI-Perf). Its test tree holds a mock server that goes further than most on realistic load behaviour.

- Language: Python (FastAPI, uvicorn)
- License: Apache-2.0
- Signs of life: 900 stars, last push 2026-11-20
- Location: `tests/aiperf_mock_server`, run as `aiperf-mock-server`. Every option also has a `MOCK_SERVER_` environment variable.
- Endpoints: OpenAI chat, completions, responses, embeddings, images, models. Anthropic `/v1/messages` (streaming, thinking, tool use). HuggingFace TGI `/generate` and `/generate_stream`. Three rerank formats. Fake Prometheus metrics in vLLM, SGLang, TensorRT-LLM and Dynamo shapes. Fake DCGM GPU telemetry.

How it works (from its README):

- Text: the README describes a pre-tokenized corpus loaded from a bundled `shakespeare.txt`, and in another section says output tokens are generated by cycling through the input tokens. Either way it states that identical inputs produce identical outputs.
- Tokenizer: bundled tiktoken `o200k_base` by default, any HuggingFace tokenizer by name, or `no-tokenizer` for character-based chunking at about 4 characters per token.
- Length and `finish_reason`: with the limit unset, output is 0.8 to 1.2 times the prompt length (minimum 16) and finishes with `stop`. With `max_tokens` set, output goes up to the limit and finishes with `length` or `stop`. With `ignore_eos` true, output is exactly `max_tokens` and finishes with `length`. `min_tokens` is honoured.
- Latency: `ttft` (default 20 ms) and `itl` (default 5 ms). Streaming waits TTFT, then ITL between tokens. Non-streaming waits `TTFT + ITL x token_count`. `fast` sets every latency to zero.
- Jitter: `ttft-jitter-cv` and `itl-jitter-cv` add lognormal noise, sampled once per request for TTFT and per token for ITL. Combined with `random-seed` the noise trace repeats across runs.
- Load-dependent penalties: `ttft_per_isl_token_ms`, `ttft_concurrency_quad_ms`, `itl_per_osl_token_ms`, `itl_concurrency_lin_ms`.
- Saturation model (`scheduler-enabled`): a virtual decode loop ticks every `scheduler-step-ms` (5 ms). Each tick advances up to `scheduler-max-batch-size` (256) requests by one token and the rest wait. Prompts are split into prefill chunks of `scheduler-prefill-chunk-tokens` (512) and each tick processes up to `scheduler-max-prefill-chunks-per-step` (8). This gives a real throughput ceiling (`max_batch_size / step_ms` tokens per second) and a TTFT cliff when prefill is the bottleneck.
- Goodput collapse (optional): past a queue-depth threshold the effective batch shrinks linearly down to a floor, so measured tokens per second drops under heavy overload.
- Streaming: honours `stream_options.include_usage` and ends with `[DONE]`. Option `emit-empty-chat-content` sends one explicit empty content chunk first.
- Reasoning: models with `gpt-oss` or `qwen` in the name emit `reasoning_content` first, sized by `reasoning_effort` (100, 250 or 500 tokens), counted against `max_tokens`.
- Failure injection: `error-rate` (0 to 100 percent) makes requests fail with HTTP 500, and `random-seed` makes the failure sequence repeatable. `inference-ready-delay-seconds` returns 503 until the delay passes. `disable-models-endpoint` returns 404 on `/v1/models`. `api-key` turns on auth checking.
- Client disconnects: a dropped streaming client is removed from the scheduler queues and counted in a metric.
- Request recording: `record-requests` writes one JSONL line per request with the real input token count and the requested output cap, plus a summary on shutdown.
- Tool calls on the chat-completions route: unconfirmed. The README mentions tool use for the Anthropic route.

Sources:

- https://raw.githubusercontent.com/ai-dynamo/aiperf/main/tests/aiperf_mock_server/README.md
- https://github.com/ai-dynamo/aiperf

## 6. NVIDIA Dynamo mocker (Rust core)

The most detailed timing and scheduling model in this survey. It plugs in as a backend engine behind Dynamo's own frontend and router, so it is a simulated engine and the HTTP layer in front of it is Dynamo's real one.

- Language: Rust core (`lib/mocker`), started with `python -m dynamo.mocker`
- License and stars: left unchecked
- Purpose (docs): "registers workers, publishes KV events, and exercises the real frontend/router/planner path without requiring GPUs."

What it simulates:

- Block-based KV cache with LRU eviction, prefix caching by block hash, chunked prefill, continuous batching, preemption under memory pressure, prefill/decode disaggregation, KV event publishing.
- Two scheduler shapes: a vLLM-style waiting-plus-running scheduler and an SGLang-style cache-aware one.
- Defaults: 16,384 KV blocks, block size 64 tokens (vLLM), 256 max concurrent sequences, 8,192 max batched tokens.

Timing:

- Default "polynomial" model: prefill time scales quadratically with token count, decode time depends on the total active KV cache size.
- "Interpolated" model: loads measured prefill and decode latencies from a profiling file and interpolates.
- An optional model backed by NVIDIA's AI Configurator.
- The v1.5.1 architecture page also lists a `fixed` timing mode with set `prefill_ms` and `decode_ms`, described as "for deterministic tests".
- Knobs: `speedup-ratio` (scale all simulated time), `decode-speedup-ratio`, `startup-time`, `kv-transfer-bandwidth` (default 64 GB/s).

Output tokens (from the example engine source at one commit):

- Each generated token is a random token id between 1000 and 2000 from the thread RNG, so the decoded text is noise and differs run to run.
- Generation always runs to `max_tokens` (default 16 when unset) and finishes with reason `length`. Cancellation gives a `cancelled` terminal chunk.
- Usage is the real prompt token count plus the generated count.
- Requests for logprobs get synthetic ones.
- A `reasoning` option emits thinking-token spans at a set ratio.

Sources:

- https://docs.nvidia.com/dynamo/user-guides/dynosim/mocker
- https://docs.nvidia.com/dynamo/reference/components/mocker-cli-reference (search excerpt)
- https://docs.nvidia.com/dynamo/v1.5.1/knowledge-base/modular-components/backends/mocker/mocker-engine-architecture (search excerpt)
- https://github.com/ai-dynamo/dynamo/blob/004cd02b/lib/backend-common/examples/mocker/src/engine.rs

## 7. vLLM: mock engine, render-only server, simulator proposal

vLLM itself now has a no-model path, built for stress testing its own frontend.

- `vllm-mock-engine` (Rust, in `rust/src/mock-engine` of the vLLM repository): an engine-side process that joins the frontend's startup handshake, "treats prefill as instant, and emits random decode tokens until each request reaches its `max_tokens`". The real vLLM frontend (Python `vllm serve` or the Rust `vllm-rs`) still does the HTTP, tokenization and detokenization.
- Knobs: `engine-count`, `output-token-chunk-size` (token ids per engine output, useful for speculative-decoding shaped tests), `vocab-size` (example 32000), `seed` (example 0), `log-requests`.
- The README warns: "Always pass `max_tokens`; the mock engine stops by length."
- It has a `seed` flag, so the random token stream is presumably repeatable. I did not run it to confirm.
- A draft PR (54572, opened 2026-08-31) adds a Dockerfile to make it a turn-key mock vLLM.
- `vllm launch render` (PR 34551, merged 2026-03-04): a GPU-free server that only does chat-template rendering and tokenization (`/v1/chat/completions/render`, `/v1/completions/render`). Generation endpoints return an error by design. llm-d-inference-sim uses this as its exact tokenizer.
- PR 61747 proposes `tools/vllm-simulator`: run the real vLLM engine on a CPU host, skip the model forward pass, and advance a simulated clock by a hardware-aware per-step latency prediction. Its merge state is unconfirmed (search excerpt only).

Sources:

- https://github.com/vllm-project/vllm/tree/main/rust/src/mock-engine
- https://github.com/vllm-project/vllm/pull/34551 (search excerpt)
- https://github.com/vllm-project/vllm/pull/54572 (search excerpt)
- https://github.com/vllm-project/vllm/pull/61747 (search excerpt)

## 8. Envoy AI Gateway: testupstream and testopenai

Envoy AI Gateway tests its data plane against two different fakes. Both are Go, Apache-2.0 (per the file headers).

`testupstream` (`tests/internal/testupstreamlib`), a header-driven fake:

- The test tells the fake what to return through request headers. `x-response-body` (base64) is the body, `x-response-status` the status code, `x-response-headers` extra headers, `x-response-type` picks plain JSON, `sse`, `sse-gzip`, `gzip` or `aws-event-stream`.
- It also asserts on what the gateway sent: `x-expected-path`, `x-expected-headers`, `x-expected-request-body`, `x-expected-host`, `x-non-expected-request-headers`, and `x-expected-testupstream-id` (compared with the `TESTUPSTREAM_ID` environment variable, so a request routed to the wrong upstream is rejected).
- Streaming: each line (or each blank-line-separated block) of the body is sent as one SSE event with a fixed 200 ms pause before each.
- With the body header unset on `/v1/chat/completions`, it picks a random line from about 40 canned ones ("This is a test.", "The quick brown fox jumps over the lazy dog.", movie quotes) and returns it with fixed usage numbers (`prompt_tokens` 1, `completion_tokens` 100, `total_tokens` 300). The pick uses a time-seeded RNG, so it varies.
- `x-fake-response: small | medium | large` returns a prebuilt body of 100, 10,000 or 100,000 repeated lines (roughly 5 KB, 500 KB, 5 MB) for benchmarking.
- Error injection is simply setting `x-response-status`.

`testopenai` (`tests/internal/testopenai`), record and replay:

- Uses go-vcr v4 cassettes (YAML) of real OpenAI and Azure OpenAI traffic. The test names the cassette in an `X-Cassette-Name` header.
- With an API key in the environment and no matching cassette, it calls the real API once and saves the result.
- Failure modes are explicit: 400 when the header is missing and nothing matches, 409 when the recorded interaction is out of date, 500 when the cassette is absent and no API key is set.
- The README says it replays "with delays faster than real platforms to keep tests fast".

Sources:

- https://raw.githubusercontent.com/envoyproxy/ai-gateway/main/tests/internal/testupstreamlib/testupstream.go
- https://raw.githubusercontent.com/envoyproxy/ai-gateway/main/tests/internal/testupstreamlib/server.go
- https://raw.githubusercontent.com/envoyproxy/ai-gateway/main/tests/internal/testopenai/README.md

## 9. Kubernetes Gateway API Inference Extension

- It uses llm-d-inference-sim as its test backend. Commit 606097f (2026-04-01) bumps the simulator image to v0.8.2 in `config/manifests/vllm/sim-deployment.yaml`, `sim-grpc-deployment.yaml`, `config/manifests/bbr/sim-deployment.yaml` and `test/testdata/test-sim-deployment.yaml`.
- On main today the `config` directory lists only `crd`. A search excerpt shows the same sim-deployment manifest living in `llm-d/llm-d-router` with image v0.10.2 and arguments for model name, port 8000, `max-loras 2` and one LoRA module.
- Lesson from PR 1112 (July 2025): a new simulator release with breaking changes broke the e2e tests, so they pinned the simulator to a specific version.

Sources:

- https://github.com/kubernetes-sigs/gateway-api-inference-extension/commit/606097f21c3425c62ca81c5a3f97835f21ccdfef (search excerpt)
- https://github.com/kubernetes-sigs/gateway-api-inference-extension/pull/1112 (search excerpt)
- https://github.com/llm-d/llm-d-router/blob/main/config/manifests/vllm/sim-deployment.yaml (search excerpt)

## 10. Helicone AI Gateway mock-server (Rust)

- Language: Rust (axum). License: Apache-2.0 (workspace `Cargo.toml`). Workspace version 0.2.0-beta.30.
- Location: `crates/mock-server`. Default port 5150.
- Routes: OpenAI `/v1/chat/completions`, Anthropic `/v1/messages`, Gemini's OpenAI-compatible path, Bedrock `converse`, plus fakes of Helicone's own control plane and object storage.
- Text: a fixed JSON stub read from `ai-gateway/stubs/openai/chat_completion.json` at compile time. Every request gets the same body.
- Latency: per-provider setting (default 60 ms). Each request sleeps a uniformly random time within 10 ms either side of it, from the thread RNG.
- Use: k6 load tests. The benchmarks README reports 3,000 requests per second sustained with the mock at a 60 ms median, and uses the gap between total time and provider time to show gateway overhead under 1 ms.
- Streaming, tool calls and failure injection: the OpenAI handler I read returns only the JSON stub.

Sources:

- https://raw.githubusercontent.com/Helicone/ai-gateway/main/crates/mock-server/src/lib.rs
- https://raw.githubusercontent.com/Helicone/ai-gateway/main/crates/mock-server/src/routes/openai.rs
- https://raw.githubusercontent.com/Helicone/ai-gateway/main/crates/mock-server/src/routes/mod.rs
- https://github.com/Helicone/ai-gateway/blob/main/benchmarks/README.md (search excerpt)

## 11. Kong AI Gateway (ai-proxy plugin specs)

- Kong's Lua spec tests declare a mock upstream as an nginx `server` block inside the test (`fixtures.http_mock`), with `content_by_lua_block` handlers.
- Each handler checks the auth header and the body, then prints a fixture file: `good.json`, `bad_request.json` (400), `unauthorized.json` (401), `bad_upstream_response.json`.
- One URL path per scenario: `/llm/v1/chat/good`, `/llm/v1/chat/bad_upstream_response`, and so on.
- The streaming spec prints a preset list of event chunks as SSE. The encoding spec switches on an `x-test-type` request header (`200`, `200_FAULTY`, `401`, `500`, `500_FAULTY`) and gzips the body.
- A DNS mock points `api.openai.com` at 127.0.0.1 so the unmodified driver hits the fake.

Sources (search excerpts of the spec files):

- https://github.com/Kong/kong/blob/58f2daa5/spec/03-plugins/38-ai-proxy/02-openai_integration_spec.lua
- https://github.com/Kong/kong/blob/58f2daa5/spec/03-plugins/38-ai-proxy/08-encoding_integration_spec.lua
- https://github.com/Kong/kong/blob/58f2daa5/spec/03-plugins/38-ai-proxy/09-streaming_integration_spec.lua

## 12. inference-perf mock client

- Language: Python. License: Apache-2.0. 281 stars, last push 2026-11-19.
- `MockModelServerClient` replaces the HTTP client inside the benchmark tool, so nothing goes over the network.
- Behaviour: sleeps `mock_latency` (default 1 second) and records a successful request with 0 input and 0 output tokens. If the configured timeout is shorter than the mock latency it sleeps for the timeout and records a timeout error.
- The repository also has a `mock_datagen.py` and an e2e config named `e2e_simple_mock_client.yaml` (file names seen in a code search).

Source:

- https://raw.githubusercontent.com/kubernetes-sigs/inference-perf/main/inference_perf/client/modelserver/mock_client.py

## 13. Server-side test modes: does a real server ship a no-model mode?

LocalAI:

- Ships a mock gRPC backend for its own e2e tests at `tests/e2e/mock-backend/main.go` (Go). The real LocalAI HTTP server runs in front of it.
- Behaviour is triggered by magic words in the prompt: `MOCK_ERROR` (error), `MOCK_ERROR_IMMEDIATE` (stream fails at once), `MOCK_ERROR_MIDSTREAM` (streams "Partial resp" and then fails), `AUTOPARSER_TOOL_CALL`, `SINGLE_TOOL_CALL`, `MULTI_TOOL_CALL`, `AUTOPARSER_CONTENT`, `AUTOPARSER_THINKING_CONTENT`, `ECHO_LOAD_PARAMS`.
- Default reply contains "mocked response". Token counts are fixed numbers (for example 10 completion and 5 prompt).
- The test for the mid-stream failure checks that the HTTP status is 200, the body contains a structured error event with "simulated mid-stream failure", and the stream still ends with `[DONE]`.

Triton Inference Server:

- Has an `identity` backend that copies input tensors to output, used by `qa/L0_perf_nomodel` to measure pure server overhead.
- A config parameter `execute_delay_ms` adds a fixed delay per execution (2000 in one QA config).
- It speaks Triton's tensor protocol, so it is a no-model mode for the server and not an LLM-shaped fake.

llama.cpp server:

- Its test suite runs the real `llama-server` with a tiny real model, `stories260K.gguf` from `ggml-org-dev/stories260K`, with settings such as seed 42, context 512, `n-predict 64`. So the approach is "smallest possible real model", and real inference still runs.
- The README warns that parallel scenarios can fail on hosts faster than the CI runners, which is the kind of timing flakiness a fake avoids.

Ollama:

- Issue 728 asked for a dummy model that splits the prompt on whitespace and echoes it back. The maintainer's answer was to write a mock or stub in the test instead, and the issue was closed.
- Third-party `kinduff/ollama-mock` (Go, single binary): deterministic answers that vary by prompt (prose, lists, code, tool calls, JSON, thinking), per-model realistic speeds, and a `latency` time-scale flag where 1 is real speed, 0 is instant and 0.1 is ten times faster. Reported durations in responses stay realistic at any scale.

vLLM: see section 7 (mock engine and render-only server).

TGI: the only TGI-shaped fake I found is AIPerf's mock server, which implements `/generate` and `/generate_stream`.

Sources:

- https://github.com/mudler/LocalAI/blob/352b7ec6/tests/e2e/mock-backend/main.go
- https://github.com/mudler/LocalAI/blob/352b7ec6/tests/e2e/mock_backend_test.go (search excerpt)
- https://github.com/triton-inference-server/server/blob/df3dec34/qa/L0_perf_nomodel/run_test.sh (search excerpt)
- https://github.com/triton-inference-server/server/blob/399a7959/qa/L0_metrics/identity_delay/config.pbtxt (search excerpt)
- https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/tests/README.md
- https://github.com/jmorganca/ollama/issues/728 (search excerpt)
- https://github.com/kinduff/ollama-mock (search excerpt)

## 14. Standalone fake OpenAI servers (small projects)

The lead names `fake-openai-server` and `openai-mock-server` turned out to be a crowd of small projects. Two are Rust and worth a closer look.

`larsakerlund/llmock` (Rust, MIT, README fetched in full):

- A single Rust binary that emulates OpenAI (chat and Responses), Anthropic and Gemini wire formats, each under its own path prefix.
- Text: YAML fixture rules tried top to bottom. A rule matches on `user_contains` (substring of the last user message) or exact `model`, and an empty match is the fallback. With no fixtures file it answers "This is a mock response from llmock."
- Usage and `finish_reason`: set per rule (`stop`, `length`, `tool_calls`, `content_filter`).
- Latency: `ttft_ms`, `inter_token_ms`, `burstiness` (0 to 1) and `chunk_by` (`word`, `char`, or a number of characters). With probability `burstiness` a gap is zero, otherwise it is an exponential pause sized so the average gap stays at `inter_token_ms`. Built-in defaults are per model family, "derived from measurements of real APIs".
- Non-streaming calls wait the same total time a stream would take.
- Faults: `truncate` (drop the connection after N deltas, with the final chunk and `[DONE]` left unsent), `malformed` (emit a broken SSE frame after N deltas), `hang` (stall for `hold_ms` after N deltas).
- Errors: any status with a provider-faithful error envelope, returned before the stream starts.
- Tool calls: per rule, streamed as an opening delta (id, type, name) followed by argument fragments at the `chunk_by` granularity. Multiple calls supported.
- Streaming: supports `stream_options.include_usage`.
- Record and replay: proxies a real provider once, saves the bytes and the delay of each network chunk, and replays them. `replay-speed` scales timing (0 is instant).
- Repeatability: a `deterministic` flag turns off burstiness and jitter.

`pinkfuwa/mock-openai` (Rust, README seen as a search excerpt, license unchecked):

- Aimed at load testing: claims over 10,000 requests per second, with HTTP/2 and TLS support.
- Text comes from a pool of pre-generated content (`pregen-count`, default 4096).
- Length is drawn with `token-mean` (default 256) and `token-stddev` (default 64).
- Latency: `response-delay-ms` between SSE chunks (default 0).

Others seen in search excerpts:

- `go-go-golems/openai-mock-server` (Go): chat and Responses, SSE, and a rule engine covering tools, error injection and delays.
- `theblixguy/llm-mock-server` (TypeScript): `server.when("hello").reply("Hi there!", { latency: 50, chunkSize: 5 })`, same rule served on OpenAI chat, Anthropic and OpenAI Responses routes.
- `patrickjm/openai-mock-api` (TypeScript): YAML conversation flows with exact, fuzzy, regex and contains matching, tool calls, and token counts from tiktoken.
- `freakynit/mock-openai-server` (Node): chat, embeddings, images and audio driven by a `config.yaml`.
- CopilotKit `llmock` (Node): fixture-driven, multi-provider SSE, built to be shared by several processes in end-to-end tests.

Sources:

- https://raw.githubusercontent.com/larsakerlund/llmock/main/README.md
- https://github.com/pinkfuwa/mock-openai/blob/main/README.md (search excerpt)
- https://github.com/go-go-golems/openai-mock-server (search excerpt)
- https://github.com/theblixguy/llm-mock-server (search excerpt)
- https://github.com/patrickjm/openai-mock-api/blob/main/README.md (search excerpt)
- https://github.com/freakynit/mock-openai-server (search excerpt)
- https://github.com/NathanTarbert/llmock (search excerpt of a copy of the CopilotKit README)

## 15. Leads with thin results

- Portkey gateway: the test file `src/tests/common.test.ts` loops over providers and skips each one whose API key is unset, so that suite talks to real providers. People testing Portkey point its `custom_host` setting at their own local mock (seen in two third-party write-ups).
- agentgateway (Rust): a DeepWiki page describes mock helpers under `crates/agentgateway/src/test_helpers/` for external processing, rate limiting, external auth and OTel, plus golden tests for LLM request and response translation in `crates/llm/src/golden_tests.rs`. That is a secondary source. A simulated LLM backend there is unconfirmed.
- LangDB AI gateway (Rust): the search surfaced only provider code that accepts a custom endpoint. A bundled mock backend is unconfirmed.
- Plano (formerly archgw), bricks-llm, Cloudflare AI Gateway, OpenRouter-style proxies, llmperf, Locust LLM examples, GenAI-Perf: my searches returned nothing specific to a mock backend, or I ran out of budget before reaching them. All unconfirmed.

Sources:

- https://github.com/Portkey-AI/gateway/blob/669825cb/src/tests/common.test.ts (search excerpt)
- https://deepwiki.com/agentgateway/agentgateway/14.4-testing-framework (search excerpt)

## 16. What I could not confirm

- llm-d-inference-sim: the exact chat streaming chunk sequence (role chunk, `[DONE]`) and whether it can cut a stream midway. I read the docs and left the Go source unread.
- AIPerf mock server: tool calls on `/v1/chat/completions`, and which of the two README descriptions of text generation matches the code.
- vLLM: whether the mock engine's `seed` gives byte-identical output, and whether PR 61747 (simulator) has merged.
- Dynamo mocker: license, stars, and whether current releases still emit random token ids as in the commit I read.
- LiteLLM: the license text (GitHub says "Other"), the chunk size of mock streams in current code, and the contents of its e2e perf harness.
- Envoy AI Gateway, Kong, LocalAI, Triton: repository licenses and activity went unchecked. Envoy's Go files state Apache-2.0 in their headers.
- Portkey, agentgateway, LangDB, Plano or archgw, bricks-llm, Cloudflare AI Gateway, OpenRouter-style proxies, llmperf, Locust examples, GenAI-Perf, TGI: a bundled mock or simulator backend is unconfirmed for all of them.
- Items marked "search excerpt" were read as excerpts returned by the search tool. Treat their details as lower confidence than the pages fetched in full.

## 17. Patterns across projects

Five shapes of fake, and what each one tests:

- In-process dummy provider (TensorZero `dummy`, LiteLLM `mock_response`, inference-perf mock client). Fastest. Skips the HTTP client, so it cannot catch SSE parsing or connection bugs.
- Standalone OpenAI-compatible fake server (llm-d-inference-sim, GuideLLM, AIPerf, llmock, Helicone mock-server, BerriAI example endpoint). Tests the real wire path. This is the shape that fits a gateway under test.
- Mock engine behind the real frontend (vLLM mock engine, Dynamo mocker, LocalAI mock backend). Tests the server's own HTTP and scheduling code.
- Record and replay (Envoy `testopenai` cassettes, TensorZero `provider-proxy`, llmock cassettes). Highest fidelity to a real provider's bytes, at the cost of maintaining recordings.
- Test-scripted upstream (Envoy `testupstream`, Kong's nginx mocks). The test writes the exact response and the fake also asserts on what the gateway sent.

How a request selects behaviour:

- By model name: TensorZero (dozens of names), BerriAI endpoint (`429`, `random_sleep`, `degraded`), AIPerf (reasoning by name).
- By request header: Envoy `x-response-*`, llm-d `X-Return-Error`, Kong `x-test-type`.
- By magic words in the prompt: LocalAI `MOCK_ERROR_MIDSTREAM`, llmock `user_contains`.
- By server flags or config: llm-d, GuideLLM, AIPerf.
- By a runtime admin call: llm-d `POST /admin/config`, which lets one test flip failure rate or latency without a restart.

How text is chosen:

- One fixed sentence (TensorZero default, LiteLLM, BerriAI endpoint, Helicone, LocalAI). The current "pong" backend is this.
- Echo of the last user message (llm-d echo mode, TensorZero `echo`).
- Random pick from canned lines (llm-d random mode, Envoy testupstream).
- Seeded filler sized to an exact token count (GuideLLM with Faker, AIPerf with a corpus).
- Random token ids (vLLM mock engine, Dynamo mocker), where the real frontend detokenizes noise.
- Fixture rules written by the tester (llmock and most small servers).

Token counting and `finish_reason`:

- Many fakes hard-code usage: TensorZero 10 in, LiteLLM 10 in and 20 out, BerriAI 9 and 12, Envoy 1 and 100, Helicone a stub file.
- The simulators count with a cheap regex tokenizer (llm-d, GuideLLM) or about 4 characters per token (AIPerf option), with a real tokenizer as an opt-in.
- llm-d and AIPerf treat `max_tokens` the way a real server does: cap the output, report `length` when the cap is hit and `stop` otherwise, and support `min_tokens` and `ignore_eos` for exact lengths. GuideLLM caps the length and still reports `stop`. The engine mocks (vLLM, Dynamo) always run to `max_tokens` and report `length`. The fixed-text fakes ignore the limit.
- llm-d also enforces a context window and returns 400 when the prompt overflows it.

Latency:

- The standard model has two knobs: time to first token, then a gap per token. Non-streaming waits `TTFT + gap x tokens`.
- Defaults split two ways: zero (llm-d, llmock under test flags, AIPerf `fast`) or small realistic values (GuideLLM 150 and 10 ms, AIPerf 20 and 5 ms, Helicone 60 ms).
- Jitter shapes: Gaussian with clamps (llm-d, GuideLLM), lognormal (AIPerf), bursty with exponential pauses (llmock), uniform (Helicone).
- Prompt-length dependence: llm-d per-token prefill with a cache-hit discount, AIPerf per-input-token penalty, Dynamo quadratic prefill.
- Load dependence: llm-d linear factor up to the concurrency limit, GuideLLM semaphore, AIPerf batch-step scheduler with a throughput ceiling and optional collapse, Dynamo full scheduler with KV memory pressure.
- A single time-scale knob shows up repeatedly (ollama-mock `latency`, Dynamo `speedup-ratio`, llmock `replay-speed`, AIPerf `fast`): keep the realistic shape, run it instantly in CI.

Failure injection:

- Deterministic triggers suit tests: a status header (llm-d, Envoy), a model name (TensorZero `error`, BerriAI `429`), fail after N requests (GuideLLM), fail on every even call (TensorZero `flaky_`).
- Probabilistic rates suit load and chaos runs, and the good ones take a seed (AIPerf `error-rate` with `random-seed`, llm-d `failure-injection-rate` with `seed`).
- Mid-stream failures are the rare, valuable ones: TensorZero `err_in_stream` and `fatal_stream_error` at a fixed chunk index, LocalAI `MOCK_ERROR_MIDSTREAM`, llmock `truncate`, `malformed` and `hang`, LiteLLM `mock_streaming_error`.
- Slow paths: TensorZero `slow` and `slow_second_chunk`, BerriAI `degraded`, llmock `hang`, LiteLLM `mock_timeout`.
- Readiness: llm-d `startup-duration`, AIPerf `inference-ready-delay-seconds`, Dynamo `startup-time` return "not ready" for a while.

Streaming details that the careful ones get right:

- A final chunk with an empty delta and the finish reason, then a usage chunk with empty `choices` only when `stream_options.include_usage` is set, then `[DONE]` (GuideLLM, read in source).
- Tool calls as an opening delta with id, type and name, followed by argument fragments (GuideLLM, llmock, TensorZero).
- Awkward but legal splits: tool name split across chunks (TensorZero `tool_split_name`), arguments one character at a time (GuideLLM), invalid JSON arguments (TensorZero `invalid_tool_arguments`).
- Simple fakes often end the stream by closing the connection (BerriAI endpoint), which a strict client may treat as a cut stream.

Repeatability:

- Fixed text is repeatable for free. Everything random needs an explicit seed, and the default matters: llm-d seeds from the clock unless told otherwise, Envoy's canned pick is time-seeded.
- Process-wide counters (TensorZero `flaky_`) make results depend on test order and parallelism.

Operational lessons:

- LiteLLM moved its client-supplied mock parameters behind explicit opt-ins and now rejects them with a 400 when disabled, because silently ignoring a mock request made fallback tests pass without testing anything.
- The Gateway API Inference Extension pins its simulator version after a simulator release broke its e2e suite.
- Routers and autoscalers need more than completions: llm-d and AIPerf both fake the Prometheus metrics endpoint, and llm-d lets a test set the metric values directly.

Ideas most worth copying for a Rust fake backend:

- Pick behaviour by model name, with a per-request header override for status codes. Confidence: high, it is the most common pattern and keeps tests self-describing.
- Generate deterministic text sized by a cheap regex tokenizer, with echo and seeded-filler modes, honest `usage`, and `length` versus `stop` driven by `max_tokens`. Confidence: high, llm-d and AIPerf show gateways need this for token accounting tests.
- Model latency as TTFT plus per-token gap, default zero, with one global time-scale and an optional concurrency limit. Confidence: high for the two knobs, medium for the load model since only load tests need it.
- Script failures at exact points: status before the stream, error or hang or cut at chunk N, fail every Nth call. Confidence: high, mid-stream faults are where proxies break and few fakes offer them.
- Ship awkward streaming fixtures: split tool names, fragmented arguments, invalid JSON arguments, usage chunk on request. Confidence: high, TensorZero's list reads like a bug history.

