---
produced: 2026-11-22
title: "Fake LLM test models built into frameworks and SDKs - Pydantic AI TestModel and FunctionModel, LangChain fakes, LlamaIndex MockLLM, Vercel AI SDK mocks, DSPy DummyLM, OpenAI Agents ScriptedModel, Genkit mockModel and echoModel, Haystack MockChatGenerator, LangChain4j MockChatModel, AutoGen ReplayChatCompletionClient, LiteLLM mock_response, Rust rig and swiftide mocks - how each one produces output, streams, calls tools, reports usage"
---

# Fake and test models that ship inside LLM frameworks and SDKs

This is a survey of the stand-in models that frameworks ship for tests, with the focus on how each one decides what to say. The goal is to pick ideas for a Rust in-process fake that sits behind an OpenAI chat-completions surface (plain JSON and SSE, with tool calls).

## How to read this

- Unless a bullet says otherwise, a fact below comes from a page I opened on 2026-11-22: either the source file on the main branch, or the official docs page.
- Where I only saw something in a search result excerpt and did not open the page myself, the bullet says "search excerpt only".
- Where I looked and could not confirm something, the bullet says "not confirmed". There is a full list of those at the end.
- Several of these projects moved fast this year. Three things that differ from older write-ups: the OpenAI Agents SDK now ships a public `ScriptedModel` (the old `tests/fake_model.py` path returns 404), the Vercel AI SDK docs now show `MockLanguageModelV4`, and LangChain4j replaced `ChatModelMock` with a new `langchain4j-test` module.

## Pydantic AI - `TestModel`, `FunctionModel`, `ALLOW_MODEL_REQUESTS`

This is the most interesting one for us, because `TestModel` needs zero scripting: it reads the tool and output schemas from the request and builds valid data on its own.

### `TestModel`

- Family: schema-driven auto-pilot.
- Knobs (constructor): `call_tools` (list of tool names or `'all'`, default `'all'`), `custom_output_text`, `custom_output_args`, `seed` (default `0`), `model_name` (default `'test'`), `profile`, `settings`.
- It also records `last_model_request_parameters`, so a test can inspect which tools and output tools the agent offered on the last request.
- The exact decision rule for one request, in order:
    1. If the request asks for provider-native tools, raise `UserError('TestModel does not support built-in tools')`.
    2. Work out the tools to call: every visible function tool when `call_tools='all'`, otherwise the named ones. An unknown name raises `UserError`.
    3. Work out the final-output plan: `custom_output_text` wins, then `custom_output_args`, then plain text if the agent allows text output, then the output tool, then plain text.
    4. If there are tools to call and the message history holds zero model responses so far (so this is the first model turn), reply with one tool call per tool, all in a single response. Arguments come from the schema generator below.
    5. Otherwise look at the last request. If it holds tool returns marked as retried (the tool or output validator asked for a retry), call just those tools again with freshly generated arguments.
    6. Otherwise produce the final answer. For text output: `custom_output_text` if set, else a JSON object that maps each tool name to what that tool returned (for example `{"weather_forecast":"Sunny with a chance of rain"}`), else the literal text `success (no tool calls)` when the history holds zero tool returns.
    7. For structured output: call the output tool `output_tools[seed % len(output_tools)]` with `custom_output_args` if set, otherwise with schema-generated arguments.
- Tool call ids are deterministic: `pyd_ai_tool_call_id__{tool_name}`, with `__2`, `__3` added only when the same id would repeat inside one response.
- So the whole conversation shape is fixed: turn 1 calls every tool once, turn 2 gives the final answer. It never reads the user prompt. The docs say so directly: "TestModel isn't doing anything clever to extract values from the prompt, so these values are hardcoded."

### How `TestModel` generates schema-valid data (`_JsonSchemaTestData`)

The class docstring says it "tries to generate the minimal viable data for the schema". The rules, checked in this order:

- A boolean schema (`true`/`false`): one character.
- `const`: the const value.
- `enum`: `enum[seed % len(enum)]`.
- `examples`: `examples[seed % len(examples)]`.
- `allOf`: merge the members into one schema (properties and `required` are combined, bounds take the tighter value, nested `$ref` and `allOf` are followed once each so a cycle ends), then generate from the merged schema.
- `$ref`: look it up in `$defs`, or as a local JSON pointer, and generate from the target.
- `anyOf`: `anyOf[seed % len(anyOf)]`.
- `oneOf` without a sibling `type`: member `seed % len`, and the discriminator property is forced into `required` so validation can pick the member.
- A `type` list: `types[seed % len(types)]`.
- `object`: only the required properties get values. If `additionalProperties` is a truthy schema it adds one key named `additionalProperty`. If `minProperties` is still unmet it adds optional properties (skipping any that contain a `$ref`, to avoid endless recursion), then numbered `additionalProperty_N` keys.
- `string`: if `format` is known and the value fits the length bounds, a canned value per format: `date` is 2024-01-01 plus `seed` days, `date-time` is that date plus `T00:00:00Z`, `time` is `00:00:SSZ`, `duration` is `PT{seed}S`, `email` is `user{seed}@example.com`, `uri` is `https://example.com/{seed}`, `uuid` is built from the seed as an integer, `ipv4` is `192.0.2.{seed % 256}`, `ipv6` is `2001:db8::{seed}`. Otherwise `minLength` copies of one character, or the empty string when `maxLength` is 0, or a single character. The character works like Excel column names over letters, digits and punctuation, so seed 0 gives `a`.
- `integer`: with both bounds, `minimum + seed % span`; with only a minimum, `minimum + seed`; with only a maximum, `maximum - seed`; with neither, `seed` itself (so `0` by default). Fractional bounds are moved to the nearest integer inside them. Then the value is snapped to `multipleOf` if that keeps it in bounds.
- `number`: the same idea as a float, `0.0` by default.
- `boolean`: `bool(seed % 2)`, so `False` by default.
- `array`: one item per `prefixItems` entry (or per entry of an old-style `items` list), then padding up to `minItems`, otherwise exactly one item when an `items` schema exists and `maxItems` allows it. With `uniqueItems`, the seed goes up by one after each item so the items differ.
- `null`: `None`.
- Any other `type` raises `NotImplementedError` with a message asking for a PR.
- The string generator reads `format`, `minLength` and `maxLength` only. I saw no code that reads `pattern`, so a string field with a regex pattern gets a plain character and would probably fail validation. That is my reading of the source. The docs only promise the data "should pass Pydantic's validation in most cases".
- The one `seed` drives every choice, so the same schema and seed always give the same data, and changing the seed gives a different but still valid variant.

### Streaming, usage, guard rails, and what the docs say

- Streaming: it builds the complete response first, then replays it. Text is split on single spaces with each word keeping its trailing space. If the text is a single word longer than 2 characters it is cut in half so there are still two chunks. An empty text delta goes out first. A tool call goes out as one whole event, not as argument fragments.
- Cancelling the stream makes the fake raise the same kind of error a closed HTTP connection gives a real provider.
- Usage is estimated, not fixed: input tokens are 50 (a flat overhead the source comment says matches Gemini and OpenAI) plus an estimate for every request part in the history, output tokens are an estimate for the response parts, and a tool call costs 1 plus the estimate of its JSON arguments. The estimate is a split on whitespace and punctuation. In the docs example the first response reports `input_tokens=60, output_tokens=7`.
- Metadata: `model_name='test'`, `provider_name='test'`.
- Guard rail: `pydantic_ai.models.ALLOW_MODEL_REQUESTS` is a global flag, default `True`. Set it to `False` in the test module and every real model refuses to send. The test models (`TestModel`, `FunctionModel`, `TestEmbeddingModel`, `TestImageGenerationModel`) are exempt. There is a context manager `override_allow_model_requests(...)` and a function `check_allow_model_requests()` that custom models are told to call at the top of every method that sends a request.
- Swapping the model in: `agent.override(model=TestModel())` replaces the model inside application code without touching call sites. `capture_run_messages()` lets the test read the full message exchange afterwards.
- Both test models also implement the batch API in memory: the batch runs as soon as it is created.
- What the docs say it is good for: "the simplest and fastest way to exercise most of your application code", with zero setup.
- What the docs say it is bad for: "There's no ML or AI in `TestModel`, it's just plain old procedural Python code", the data "won't look pretty or relevant", and it cannot emulate provider-executed native tools. Their own example shows the weak spot: the generated date is always in the past, so one branch of the tool never runs, and they switch to `FunctionModel` to reach it. The class docstring adds: "How useful this model is will vary significantly."

### `FunctionModel`

- Family: user function.
- You pass `function(messages, info) -> ModelResponse` for normal requests and optionally `stream_function(messages, info)` for streamed ones. Either can be sync or async, and a callable object works when the fake needs state between calls.
- `info` is an `AgentInfo` with `function_tools`, `output_tools`, `allow_text_output`, `model_settings`, `model_request_parameters`, `instructions`. So the function can see the tool schemas and decide what to call.
- Tool calls: return `ModelResponse(parts=[ToolCallPart('weather_forecast', args)])`. The docs example branches on `len(messages) == 1` to tell the first turn from the tool-result turn.
- Streaming: the stream function yields either strings (text chunks) or `dict[int, DeltaToolCall]` where each `DeltaToolCall` holds a piece of the tool name, a piece of the JSON arguments, and the call id. So here the test controls the exact fragmenting of tool arguments. It must yield at least one item or the model raises `ValueError`.
- Usage: if the function leaves usage empty, the same estimator as `TestModel` fills it in.
- Model name defaults to `function:{function_name}:{stream_function_name}`.

Sources:

- https://raw.githubusercontent.com/pydantic/pydantic-ai/main/pydantic_ai_slim/pydantic_ai/models/test.py
- https://raw.githubusercontent.com/pydantic/pydantic-ai/main/pydantic_ai_slim/pydantic_ai/models/function.py
- https://raw.githubusercontent.com/pydantic/pydantic-ai/main/pydantic_ai_slim/pydantic_ai/_utils.py
- https://ai.pydantic.dev/testing/
- https://ai.pydantic.dev/api/models/base/

## LangChain (Python) - the `Fake*` models in `langchain_core`

All of these live in `langchain_core.language_models`. Most are scripted lists, one is an echo, one is a constant.

- `FakeListLLM` (text completion): takes `responses: list[str]`. Each call returns `responses[i]` and moves `i` forward. After the last one `i` goes back to 0, so it cycles forever. Its `sleep` field is ignored here and only used by subclasses.
- `FakeStreamingListLLM`: same list. `stream()` first runs a normal invoke, then yields the answer one character at a time. `sleep` is the pause between characters. `error_on_chunk_number=N` raises `FakeListLLMError` when it reaches chunk N, which is how you test a stream that dies halfway.
- `FakeListChatModel`: list of strings, cycles the same way. Streaming is one character per chunk, with the same `sleep` and `error_on_chunk_number` (raises `FakeListChatModelError`). The last chunk is tagged `chunk_position="last"`. `batch` is overridden to run in order so the list position stays predictable.
- `FakeMessagesListChatModel`: list of full `BaseMessage` objects, cycles, optional `sleep` before each reply. Because the items are whole messages, they can hold tool calls.
- `FakeChatModel`: always answers the fixed text `fake response`.
- `GenericFakeChatModel`: takes `messages`, an iterator of `AIMessage` or strings, and calls `next()` on it for every request. An iterator does not cycle, so when it runs dry the call fails (the docs warn that "an exhausted iterator cannot be reused" and later tests fail once it runs out).
    - Streaming: it gets the whole message, then splits the text with the regex `(\s)` so words and each whitespace character arrive as separate chunks, and it fires `on_llm_new_token` for each one so callback code is exercised.
    - For the legacy `function_call` entry in `additional_kwargs` it splits string values on commas and sends each piece as its own chunk. Other `additional_kwargs` go out as one chunk.
    - In the `_stream` code I read, only `content` and `additional_kwargs` are chunked. I saw no code there that turns the `tool_calls` field into tool-call chunks.
    - It lacks `bind_tools`, which `create_agent` calls. The official docs tell you to subclass it and return `self` from `bind_tools`.
- `ParrotFakeChatModel`: echo. It returns the last input message object as the reply, and raises `ValueError` on an empty list.
- Tool-calling fake: LangChain's own agent tests use a private `FakeToolCallingModel` (in the test tree, not in the published package). Its rule: `tool_calls` is a list of lists, one inner list per turn, picked by a call counter (modulo the list length, so it cycles, except when the request has `response_format`, where turns past the end get zero tool calls). The reply text is every input message's text joined with `-`, which makes the reply a fingerprint of the whole conversation the model was sent. If the request has `response_format` and the turn has zero tool calls, it replies with `structured_response` dumped as JSON. Message ids are the call counter as a string. `bind_tools` records a cut-down tool spec in either OpenAI or Anthropic shape.
- Tool calls in general: the test writes them by hand, for example `AIMessage(content="", tool_calls=[ToolCall(name="get_weather", args={"city": "Tokyo"}, id="call_1")])`, followed by a second scripted message for the final answer. Arguments are never generated from the schema.
- Structured output: nothing special. The test scripts JSON text or a tool call.
- Usage and metadata: the fakes in `langchain_core` report nothing unless the scripted `AIMessage` holds it.
- Guard rails: I found no global switch that blocks real model requests.
- What the docs say: "By replacing the real LLM with an in-memory fake ... you can script exact responses (text, tool calls, and errors) so tests are fast, free, and repeatable without API keys." They recommend `GenericFakeChatModel` "when you need exact control of each turn, such as tool calls" and `FakeListChatModel` for text-only tests, and point to integration tests for real model behaviour.

Sources:

- https://raw.githubusercontent.com/langchain-ai/langchain/master/libs/core/langchain_core/language_models/fake.py
- https://raw.githubusercontent.com/langchain-ai/langchain/master/libs/core/langchain_core/language_models/fake_chat_models.py
- https://raw.githubusercontent.com/langchain-ai/langchain/master/libs/langchain_v1/tests/unit_tests/agents/model.py
- https://docs.langchain.com/oss/python/langchain/test/unit-testing

## LlamaIndex - `MockLLM`, `MockFunctionCallingLLM`, `MockEmbedding`

- `MockLLM(max_tokens=None)`:
    - With `max_tokens` set, `complete()` returns the word `text` repeated `max_tokens` times, joined by spaces. With `max_tokens=256` you get 256 copies of `text`.
    - Without `max_tokens`, `complete()` returns the prompt itself. So it is an echo.
    - `stream_complete()` without `max_tokens` yields the prompt one character at a time. With `max_tokens` it yields `max_tokens` chunks, each with the delta `text ` (the last one is `text`).
    - `metadata.num_output` is `max_tokens`, or `-1`.
    - `MockLLM` itself defines only `complete` and `stream_complete`. Chat calls are handled by its `CustomLLM` base class, which I did not read, so the exact text it echoes for a chat call is not confirmed.
- Why it looks like that: the docs present `MockLLM` as a token predictor for cost estimates, not as a behaviour fake. Quote: "The `max_tokens` parameter is used as a 'worst case' prediction, where each LLM response will contain exactly that number of tokens. If `max_tokens` is not specified, then it will simply predict back the prompt."
- `MockLLMWithChatMemoryOfLastCall`: same output, and it keeps `last_chat_messages` and the list `last_called_chat_function` so a test can check what would have been sent.
- `MockFunctionCallingLLM`: a user function plus a default auto-pilot.
    - You can pass `response_generator(messages, **kwargs)` that returns a `ChatMessage`, or a generator of them for streaming.
    - The default generator: if any message in the history has the tool role, reply `Tool calls complete.` If tools were passed and there are zero tool results yet, reply with one tool call per tool. With neither tools nor tool results, echo the last message's content (binary blocks are rendered as tags such as `<image>...</image>`), or a single space when there is nothing to echo.
    - How it fills tool arguments: it reads the tool's pydantic `fn_schema` and includes only the fields that have defaults, using those defaults. Required fields are left out. So this is a weaker cousin of Pydantic AI's generator: the call is well formed, and a tool with required arguments gets an incomplete call.
    - Tool call ids are `mock-tool-call-{uuid4}`, so ids differ from run to run.
    - Streaming yields one chunk per `ChatMessage` the generator produced.
- `MockEmbedding(embed_dim)`: every text and every query gets the same vector, `[0.5] * embed_dim`. The docstring says "Used for token prediction."
- Usage: none of these compute token usage. The docs pair them with the `TokenCountingHandler` callback, which counts tokens from the outside.
- Guard rails: the docs show setting it globally with `Settings.llm = MockLLM(max_tokens=256)` and `Settings.embed_model = MockEmbedding(embed_dim=1536)`. That replaces the default model everywhere, which acts as a soft guard.

Sources:

- https://raw.githubusercontent.com/run-llama/llama_index/main/llama-index-core/llama_index/core/llms/mock.py
- https://raw.githubusercontent.com/run-llama/llama_index/main/llama-index-core/llama_index/core/embeddings/mock_embed_model.py
- https://developers.llamaindex.ai/python/framework/understanding/evaluating/cost_analysis/

## Vercel AI SDK - `MockLanguageModelV2/V3/V4`, `simulateReadableStream`, `mockValues`

- Where: `ai/test` exports the mocks, `ai` exports `simulateReadableStream`. The current docs list `MockLanguageModelV4`, `MockEmbeddingModelV4`, `mockId`, `mockValues`. The `packages/ai/src/test` folder on main holds language-model mocks for V2, V3 and V4. I read V3 and V4 and they are the same code apart from the version. `MockLanguageModelV1` belongs to the older AI SDK v4 line (Mastra still imports it from a vendored copy).
- Family: user function, with scripted-list and fixed-value shortcuts.
- The rule: `doGenerate` and `doStream` each accept one of three things.
    - A function: called with the request options, returns the result.
    - A single result object: returned on every call.
    - An array of results: call N returns `array[N - 1]`. After the end it returns `undefined`. It neither cycles nor raises a clear error of its own.
    - Left unset, both default to a function that throws "not implemented".
- Recording: every call's options are pushed to `doGenerateCalls` and `doStreamCalls`, so the test can assert on prompts, tools and settings.
- Identity: `provider` defaults to `mock-provider`, `modelId` to `mock-model-id`.
- Streaming: the test writes the stream parts by hand, in the SDK's own part format, not in provider wire format: `text-start`, `text-delta`, `text-end`, then `finish` with a finish reason and usage. `simulateReadableStream({ chunks, initialDelayInMs, chunkDelayInMs })` turns the array into a `ReadableStream` and waits between chunks. Both delays default to 0, and `null` skips the timer entirely.
- `mockValues(a, b, c)` returns a function that gives the next value on each call and repeats the last one when the list runs out. `mockId` gives incrementing integer ids.
- Tool calls: the test scripts a content part `{ type: 'tool-call', toolCallId: 'call-1', toolName: 'weather', input: '{"city":"San Francisco"}' }` with finish reason `tool-calls`, then a second array entry with the final text. The real tool runs in between. The docs example asserts `model.doGenerateCalls` has length 2.
- Structured output: the mock returns JSON as text (for example `{"content":"Hello, world!"}`) and the SDK's `Output.object({ schema })` parses and validates it. The streamed variant splits the JSON across several `text-delta` parts so partial-object parsing is exercised.
- Usage: whatever the test writes, in the full shape (`inputTokens.total`, `noCache`, `cacheRead`, `cacheWrite`, `outputTokens.total`, `text`, `reasoning`).
- The docs also show `simulateReadableStream` feeding raw `data: {...}\n\n` strings into an HTTP `Response`, to fake a UI message stream endpoint.
- Guard rails: I found no global switch.
- What the docs say: "Testing language models can be challenging, because they are non-deterministic and calling them is slow and expensive", and the mocks let you "test your code in a repeatable and deterministic way without actually calling a language model provider".
- The same folder also holds `decision-mock-model-v4.ts`, `extraction-mock-model-v4.ts` and `mock-provider-v4.ts`. I saw the file names in the folder listing and did not read them.

Sources:

- https://ai-sdk.dev/docs/ai-sdk-core/testing
- https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/test/mock-language-model-v4.ts
- https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/test/mock-language-model-v3.ts
- https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/test/mock-values.ts
- https://raw.githubusercontent.com/vercel/ai/main/packages/ai/src/util/simulate-readable-stream.ts
- https://github.com/vercel/ai/tree/main/packages/ai/src/test

## DSPy - `DummyLM`

- Family: scripted list, lookup table, or replay of an example from the prompt.
- Mode 1, list of dicts: `DummyLM([{"answer": "red"}, {"answer": "blue"}])` returns the next dict on each call. When the list runs out it keeps answering with `{"answer": "No more responses"}`. So it neither cycles nor raises.
- Mode 2, dict of dicts: `DummyLM({"What color is the sky?": {"answer": "blue"}})`. It returns the value of the first key that appears as a substring of the last message. If no key matches, the reply is the bare text `No more responses`.
- Mode 3, `follow_examples=True`: it takes the first paragraph of the final message, walks back through the earlier user and assistant pairs in the prompt (the few-shot demos), and returns the assistant text of the most recent demo whose user message contains that paragraph. In effect it "answers" by copying a demo.
- The clever part: the dict of field values is rendered by the active adapter's `format_field_with_value`, so the fake's text has exactly the layout the adapter (chat or JSON) expects to parse. A source comment explains: "The reason why DummyLM needs an adapter is because it needs to know which output format to mimic."
- `reasoning=True` puts a thinking part with the text `Some reasoning` in front.
- Streaming: the engine builds the full response and converts it to events with a helper (`response_to_events`). I did not read the helper, so the chunk size is not confirmed.
- Tool calls: I saw no tool-call support in the engine. It only emits text and optional thinking.
- Usage and metadata: `input_tokens=0, output_tokens=0, total_tokens=0`, model `dummy`, finish reason `stop`.
- Caching is switched off for it, so repeated identical calls still consume the script.
- `get_convo(index)` returns the prompt and outputs of the Nth call from history.
- Same file: `dummy_rm` and `DummyVectorizer`, a fake retriever that embeds text as a bag of hashed character bigrams (seeded with `random.seed(123)`), mean-centred and normalised. That is a neat way to get embeddings that are deterministic and still rank similar texts together.

Sources:

- https://raw.githubusercontent.com/stanfordnlp/dspy/main/dspy/utils/dummies.py
- https://raw.githubusercontent.com/stanfordnlp/dspy/main/dspy/clients/engines/dummy_engine.py

## OpenAI Agents SDK (Python and JS) - `ScriptedModel`

Both SDKs now ship a public, documented test double with the same design. This is the cleanest example of a strict script.

### Python: `agents.testing.ScriptedModel`

- Family: strict scripted queue, with a per-step user function as an escape hatch.
- The rule: the constructor takes a list of steps. Each model call pops the first step (first in, first out). One call consumes exactly one step.
- A step can be any of: a list of normalized output items (shorthand), a `ModelResponse`, a `ModelStep`, the same thing as a dict, or an exception.
- Helpers build the items: `assistant_message("text")` and `function_call("get_weather", {"city": "Tokyo"}, call_id="call_1")`.
- When the script is empty and the agent calls the model again, it raises `UnexpectedModelCall` with the call and its index. So an extra turn fails the test at the exact point it happens.
- `model.assert_complete()` raises `UnconsumedModelSteps` if steps are left over. So a run that stopped early also fails. The docs tell you to end every test with it.
- `ModelStep.respond(fn)`: the function gets the recorded `ModelCall` and returns any step shape. The docs say to prefer fixed steps, "because fixed scripts make unexpected turns easier to diagnose".
- `ModelStep.raise_error(error, retry_advice=...)`: fail one call, optionally with provider-style retry advice. A retry by the runner is one more call and consumes the next step.
- `ModelStep.stream(events, output=...)`: supply the exact normalized stream events, or an async factory that builds them from the call.
- Recording: `calls`, `first_call`, `last_call`, `remaining_steps`. Each `ModelCall` holds `system_instructions`, `input`, `model_settings`, `tools`, `handoffs`, `output_schema`, `streamed`. Mutable request data is deep-copied at call time, and each accessor returns a detached snapshot.
- `enqueue()` and `extend()` add steps later. Consumed steps and call history stay for the life of the object, so the docs say to create a new `ScriptedModel` per scenario.
- Streaming: a normal step works for both streamed and non-streamed runs. The model generates the normalized start, delta, item-done and final events from the scripted output. From the source: text goes out as one `response.output_text.delta` holding the whole text, and tool arguments as one `response.function_call_arguments.delta` holding the whole JSON string. So the automatic stream is correct in shape and has exactly one delta per item.
- Tool calls: script a `function_call` item as turn 1 and an `assistant_message` as turn 2. The real SDK tool pipeline runs in between. The docs: "This pattern covers tool input validation, execution, result conversion, hooks, guardrails, and the next model turn."
- Structured output: the test scripts the JSON text. The call record exposes `output_schema` for assertions.
- Usage: a step without usage gets `Usage(requests=1)`, or the value from `set_default_usage(...)`. The default `response_id` is `resp-789`.
- Guard rails: the scripted model replaces model I/O only. Tracing is a separate boundary, so the examples pass `RunConfig(tracing_disabled=True)` to stop trace uploads when an API key is present. Testing symbols are kept out of the top-level `agents` import on purpose.
- What the docs say it is good for: "to test orchestration owned by your application and the SDK: tool execution, handoffs, guardrails, retries, streaming, session behavior".
- What the docs say it is bad for: "Do not use these utilities to test Responses API or Chat Completions request serialization, authentication headers, provider defaults, HTTP payloads, provider stream chunks". For those, keep the real adapter and control its network transport. Also: "Exact stream events are SDK-normalized events, not Responses API or Chat Completions wire chunks."
- The same module has `scripted_sandbox_session()`, a scripted fake for the sandbox (shell, file read and write) with per-step `match`, `result`, `responder`, `error`. Sibling modules `agents.realtime.testing` and `agents.voice.testing` hold scripted Realtime and Voice models.

### JS: `@openai/agents/testing` `ScriptedModel`

- Same model: one step per call, taken from the front of the queue.
- Helpers: `assistantMessage(text)`, `functionCall(name, args, { callId })`, `modelResponse(...)`, `modelError(error, retryAdvice?)`, `modelResponder(fn)`, `modelStream(events)`, `modelStreamResponder(fn)`.
- Errors: `UnexpectedModelCallError` (script empty), `UnconsumedModelStepsError` (from `assertComplete()`), `InvalidScriptedModelStepError` (bad step, or a raw stream step used by a non-streaming call), `ScriptedModelRequestAbortedError` (named `AbortError`).
- Automatic streaming from a normal step: `response_started`, then one `output_text_delta` per text part with the full text, then `response_done` with the complete output and usage.
- The output-array shorthand gets the id `scripted-response-N` and `new Usage({ requests: 1 })`.
- It honours the request's abort signal at each checkpoint. An already-aborted request does not consume a step.
- Tracing is switched off separately: `new Runner({ tracingDisabled: true })`, `NODE_ENV=test`, `OPENAI_AGENTS_DISABLE_TRACING=1`, or `setTracingDisabled(true)`.

Sources:

- https://openai.github.io/openai-agents-python/testing/
- https://raw.githubusercontent.com/openai/openai-agents-python/main/src/agents/testing/model.py
- https://openai.github.io/openai-agents-js/guides/testing/
- https://raw.githubusercontent.com/openai/openai-agents-js/main/packages/agents-core/src/testing/scriptedModel.ts
- https://raw.githubusercontent.com/openai/openai-agents-js/main/packages/agents-core/src/testing/index.ts

## Google ADK (Python) - test-only `MockModel`

- Where: `tests/unittests/testing_utils.py` in the `adk-python` repo. It is a helper for ADK's own tests, outside the published package.
- Family: scripted list.
- `MockModel.create(responses=[...], error=None, usage_metadata=None)`. Each response can be a string, a `Part`, a list of `Part`, or a full `LlmResponse`. Strings and parts are wrapped into model-role content.
- The rule: a counter starts at -1 and goes up on every call. The call yields a deep copy of `responses[counter]`. It is plain list indexing, so one call past the end raises `IndexError`.
- If `error` is set, every call raises it.
- Recording: every `LlmRequest` is appended to `requests`.
- Streaming: `generate_content_async` yields one `LlmResponse` per call whatever the `stream` flag says. For live (bidirectional) mode, `connect()` gives a `MockLlmConnection` that replays all responses in order and then waits until closed.
- Tool calls: script a `Part` holding a function call, then a text part for the next turn. Arguments are hand-written.
- Usage: only the `usage_metadata` you pass in.
- It registers the model name `mock`.
- A second seam ADK documents in its agent class: `before_model_callback` may return an `LlmResponse`, and "a truthy response skips the model call". ADK's own callback tests use that to answer without reaching the model (search excerpt only for the tests).
- A testing reference inside the repo (`.agents/skills/adk-agent-builder/references/testing.md`) tells users to write their own small `BaseLlm` subclass that pops canned strings. Search excerpt only.

Sources:

- https://raw.githubusercontent.com/google/adk-python/main/tests/unittests/testing_utils.py
- https://github.com/google/adk-python/blob/main/src/google/adk/agents/llm_agent.py

## Genkit (JS) - `mockModel` and `echoModel`

- Where: `genkit/testing`. Both are public and documented.
- `mockModel(ai, { name, info, respond })` registers a model and returns it. The `respond` option has three shapes:
    - A single response: returned on every call.
    - A callback `(request, { sendChunk }) => response`: called once per request.
    - An array: a queue, one item per call. When the queue runs out the last item repeats. An `Error` in the queue is thrown when reached, which injects a failure on that turn.
    - Left out, the reply is empty text.
- A response can be a string, an object with any of `text`, `toolRequests: [{ name, input, ref }]`, `content`, `finishReason`, `usage`, or a full `GenerateResponseData`.
- `echoModel(ai)`: replies with `Echo: ` followed by the whole rendered request (system and tool messages get a role prefix, messages are separated by newlines, non-text parts become labelled placeholders such as `[toolRequest name(...)]`), followed by `; config: ` and the config as JSON. The docs: it "answers the question 'what would the model have seen?'"
- `echoModel` throws a clear error when the request asks for a structured output schema, because prose cannot pass validation.
- Streaming: only the callback form can stream, by calling `sendChunk('hel')`, `sendChunk('lo')` and then returning the full response. Queue items are static.
- Tool calls: `respond: () => ({ toolRequests: [{ name: 'lookup', input: { id: 1 } }] })`. `model.toolResponses` then lists the tool results the framework fed back on the next request.
- Structured output: the mock declares `supports.constrained: 'all'` by default, so the output schema arrives on `request.output.schema` for the callback to read. Setting `constrained: 'none'` exercises the other path, where the framework injects the schema into the prompt text.
- Recording: `requests`, `requestCount`, `lastRequest`, `lastRequestMessage`, `lastRequestText` (the whole conversation flattened to one string). Requests are deep-cloned when recorded.
- `respondWith(...)` swaps behaviour for later calls. `reset()` clears history and re-arms the original behaviour, meant for `beforeEach`.
- Usage: only what the response object states.
- What the docs say: testing "is about verifying your app's deterministic logic. To assess the quality of a real model's output - relevance, groundedness, safety - use Evaluation instead."
- Also in the module: `testModels`, which is the opposite thing: a conformance suite of prompts to run against real models.

Sources:

- https://raw.githubusercontent.com/firebase/genkit/main/js/ai/src/testing/mock-model.ts
- https://raw.githubusercontent.com/firebase/genkit/main/js/ai/src/testing/index.ts
- https://raw.githubusercontent.com/firebase/genkit/main/js/ai/src/testing/model-tester.ts
- https://genkit.dev/docs/testing/

## Microsoft Semantic Kernel, AutoGen, Microsoft.Extensions.AI

### Semantic Kernel

- The official guidance I found is a 2024 team blog post. It mocks `IChatCompletionService` with Moq, returning `[new ChatMessageContent(AuthorRole.Assistant, "AI response")]`, and registers the mock in the kernel's service collection.
- It says a hand-written fake class also works ("it's possible to implement that interface with fake class, but easier way would be to use mocking library of your choice"), and that mocking `HttpClient` is the third option.
- Streaming: mock `GetStreamingChatMessageContentsAsync` to return a list converted with `ToAsyncEnumerable()`. In the example that is a single chunk.
- A sample in the repo defines a private `MockChatCompletionService(modelId, mockResult)` that returns one fixed string, and the agent unit tests use `Mock<IChatCompletionService>`. Search excerpts only.
- A reusable fake model class shipped in a Semantic Kernel package: not confirmed.

### AutoGen - `ReplayChatCompletionClient`

- Where: `autogen_ext.models.replay`. Public.
- Family: scripted list.
- `ReplayChatCompletionClient(chat_completions, model_info=None)`, where each item is a string or a full `CreateResult`.
- The rule: an index walks the list. Past the end it raises `ValueError("No more mock responses available")`. `reset()` puts the index and the usage counters back to zero, which lets a test replay the same script.
- Streaming: a string is split on whitespace and each word is yielded with a trailing space (the last word without one), followed by the final `CreateResult`. A `CreateResult` item is yielded as a single object.
- Tool calls: pass a full `CreateResult` as the list item and it is returned as is. I did not open the `CreateResult` type to check its tool-call shape. `tool_choice` is ignored with a logged warning.
- Structured output: the `json_output` argument is recorded and otherwise unused.
- Usage: prompt tokens are the count of whitespace-separated words across string message contents, completion tokens the same for the reply. It keeps a running total and reports `remaining_tokens` against a budget of 10000.
- Default `model_info` says `function_calling=False`, `json_output=False`, `vision=False`, so a test that needs tools must pass its own `model_info`.
- Each `CreateResult` built from a string has `cached=True` by default (`set_cached_bool_value` changes it).
- Recording: `create_calls` lists the arguments of every `create` call.

### Microsoft.Extensions.AI - `TestChatClient`

- Where: the test project of `dotnet/extensions` (`test/Libraries/Microsoft.Extensions.AI.Abstractions.Tests`). It is a helper for the library's own tests.
- Family: user function.
- It is an `IChatClient` with three settable delegates: `GetResponseAsyncCallback`, `GetStreamingResponseAsyncCallback`, `GetServiceCallback`. Each method forwards to its delegate.
- Streaming, tool calls, usage: whatever the test's delegate returns.

Sources:

- https://devblogs.microsoft.com/agent-framework/unit-testing-with-semantic-kernel/
- https://raw.githubusercontent.com/microsoft/autogen/main/python/packages/autogen-ext/src/autogen_ext/models/replay/_replay_chat_completion_client.py
- https://raw.githubusercontent.com/dotnet/extensions/main/test/Libraries/Microsoft.Extensions.AI.Abstractions.Tests/TestChatClient.cs

## Haystack - `MockChatGenerator`

- Where: `haystack.components.generators.chat`. Public and documented. The 3.0.0 release notes (search excerpt only) announce it together with `MockTextEmbedder` and `MockDocumentEmbedder`, which "return stable, hash-derived embeddings".
- Family: all four in one component. This is the best example of a layered fake.
- The rule, picked by how you construct it:
    - Fixed: `responses="..."` or one `ChatMessage`. Same reply on every call.
    - Cycling list: `responses=[...]`. Call N returns item `N % len`, so it wraps to the start. This differs from the strict OpenAI and Rig scripts.
    - Function: `response_fn(messages)` or `response_fn(messages, tools)`. The component inspects the signature and passes the runtime tool list when the function accepts a second argument, "so the reply can depend on the runtime tool schema".
    - Echo (the default with zero configuration): it returns the text of the last message that has text. If no message has text, it returns an empty list of replies.
    - `responses` and `response_fn` together raise `ValueError`. An empty list raises too.
- Streaming: when a streaming callback is set, it rebuilds chunks from the finished reply. Text is cut with the regex `\S+\s*`, so each chunk is one word plus its trailing whitespace, in content block 0. Then each tool call goes out as one chunk in its own block, holding the tool name, id and the full JSON arguments. The last chunk gets the finish reason.
- Tool calls: pass `ChatMessage.from_assistant(tool_calls=[ToolCall(tool_name="search", arguments={"query": "Haystack"})])` as a list item, then a string for the final answer. Arguments are hand-written, or built by your `response_fn` from the tool list. The source shows an example `response_fn` that calls the first tool with empty arguments.
- Structured output: the reply is whatever text you script, so a test that needs JSON scripts JSON text or returns it from `response_fn`.
- Usage and metadata: every reply gets `model` (default `mock-model`), `index: 0`, `finish_reason` (`tool_calls` when the reply has tool calls, else `stop`) and `usage` with `prompt_tokens`, `completion_tokens`, `total_tokens`. Usage is a count of whitespace-separated words. The docstring: "an approximation (not real tokenization) intended to give downstream code realistic-looking metadata".
- It mirrors the `OpenAIChatGenerator.run` signature so it works as a positional drop-in, it accepts and ignores `generation_kwargs` and `tools_strict`, and it serialises (named functions only).
- What the docs say: "a deterministic, zero-cost drop-in replacement for real Chat Generators ... which makes it ideal for unit tests, smoke tests, and quick prototypes."

Sources:

- https://raw.githubusercontent.com/deepset-ai/haystack/main/haystack/components/generators/chat/mock.py
- https://docs.haystack.deepset.ai/docs/mockchatgenerator

## CrewAI - subclass `BaseLLM`

- The official "Custom LLM Implementation" page describes the seam: subclass `BaseLLM` and implement `call(messages, tools=None, callbacks=None, available_functions=None)` returning a string. Optional overrides are `supports_function_calling()`, `supports_stop_words()` and `get_context_window_size()`.
- That page's own "Testing Your Custom LLM" example still points the class at an HTTP endpoint, so it is about testing an adapter, not about faking a model.
- So the family is user function: you write the fake yourself as a `BaseLLM` subclass.
- A docs pull request (#8016) adds a "Try It Offline First" section with an `EchoLLM` that returns a fixed answer and reports `supports_function_calling() == False`. Search excerpt only.
- Third-party guides show a `ScriptedLLM` subclass that pops canned replies and wraps them in CrewAI's text protocol (`Thought: ... Final Answer: ...`), and drives a tool by replying with `Action:` and `Action Input:` lines first. Search excerpts only, from sites outside the project.
- A fake model class shipped in the `crewai` package: not confirmed.

Sources:

- https://docs.crewai.com/en/learn/custom-llm

## Mastra - `createMockModel` and `MockProvider`

- Where: `@mastra/core/test-utils/llm-mock`. It is a thin layer over the Vercel AI SDK mocks.
- Family: fixed text.
- `createMockModel({ mockText, objectGenerationMode, spyGenerate, spyStream, version })`. `mockText` is a string or an object. An object is turned into JSON text. `version` is `'v1'` or `'v2'` (default `'v2'`) and picks which AI SDK mock class is built.
- The rule: every call returns the same text. `spyGenerate` and `spyStream` are called with the request so the test can look at it.
- Streaming, v1: the text is split on spaces and each chunk is a word plus a space, then a `finish` part.
- Streaming, v2: `stream-start`, `response-metadata` (id `id-0`, model `mock-model-id`, timestamp 0), `text-start`, a single `text-delta` holding the whole text, `text-end`, `finish`.
- Usage is hard-coded: 10 input and 20 output tokens for generate. The v1 stream reports 3 prompt and 10 completion tokens.
- `MockProvider` is the same idea as a class, with the default text `Hello, world!`.
- The module re-exports `simulateReadableStream` and `MastraLanguageModelV2Mock`.
- Tool calls: this helper only returns text. Mastra's own agent tests also build `MockLanguageModelV1` to `V4` objects directly (search excerpt only), which is the route for scripted tool calls.

Sources:

- https://raw.githubusercontent.com/mastra-ai/mastra/main/packages/core/src/test-utils/llm-mock.ts

## Spring AI

- The reference page titled "Evaluation Testing" covers `Evaluator`, `RelevancyEvaluator` and `FactCheckingEvaluator`. All of its examples call a real model (one uses a small fact-checking model through Ollama). It is about judging answers, not about faking a model.
- Third-party guides (search excerpts only) fake the model in two ways: a Mockito mock of `ChatModel` whose `call(Prompt)` returns a canned `ChatResponse`, or a lambda `ChatModel` that pops replies from a `Deque`. They advise mocking `ChatModel` and not `ChatClient`, so advisors and the rest of the orchestration still run.
- A third-party library, `spring-ai-test-tools`, offers `VcrStubs.chatModel().respondingWith(...)` and recorded fixtures. Search excerpt only.
- A fake chat model shipped by the Spring AI project itself: not confirmed.

Sources:

- https://docs.spring.io/spring-ai/reference/api/testing.html

## LangChain4j - `MockChatModel` and `MockStreamingChatModel`

- Where: a dedicated `langchain4j-test` module, package `dev.langchain4j.test.mock`. It also holds `MockEmbeddingModel`, `MockModerationModel`, `MockScoringModel` and `MockDecisionModel` (I read only the two chat ones). The older `ChatModelMock` in core is deprecated since 1.24.0 in favour of these.
- Family: strict script with a fluent builder, plus a per-step user function.
- Building the script: every `that...` factory creates the mock with its first step and every `then...` method adds a step. Example: `MockChatModel.thatCallsTool("cancelBooking", Map.of("bookingNumber", "MS-777")).thenResponds("Booking MS-777 is cancelled.")`.
- Step kinds: `thatResponds(String)`, `thatResponds(AiMessage...)`, `thatResponds(ChatResponse)` (the whole response including metadata), `thatResponds(Function<ChatRequest, AiMessage>)`, `thatCallsTool(name, Map)` or `thatCallsTool(name, jsonString)`, `thatFails(RuntimeException)`.
- The rule at the end of the script: a request after the last step fails with an `IllegalStateException` that prints the request, "so a test notices when the code calls the model more often than the script expects".
- Opting in to repetition: `thenRepeats()` makes the last step answer every further request. Only the last step repeats, the script does not start over. A step that calls a tool is refused as the repeating step, "because an AI Service would then call the tool again and again". To test a tool-call limit you use a computed step instead.
- Tool call ids: calls without an id get `tool_call_1`, `tool_call_2`, and so on, unique within one mock. Two calls with the same fixed id in one answer are rejected, "because a provider never gives two calls of one answer the same id".
- The JSON-string form of `thatCallsTool` exists so a test can send arguments that are invalid JSON.
- Metadata: the response reports the model name from the request, and finish reason `TOOL_EXECUTION` for tool calls, else `STOP`. `withTokenUsage(...)` sets one token usage for every response. For anything else (for example `LENGTH`), give a whole `ChatResponse` as the step.
- Structured output: by default the mock reports zero capabilities, so an AI Service that returns a POJO adds format instructions to the user message. With `withSupportedCapabilities(Capability.RESPONSE_FORMAT_JSON_SCHEMA)` the service asks for a JSON schema response format instead. The mock only reports the capability. It does not check answers against the schema.
- Recording: `requests()`, `request()` (exactly one expected), `lastRequest()`.
- `MockStreamingChatModel` streams "the way a provider streams it": first thinking, then text, then tool calls, each tool call as partial tool calls followed by the complete tool call.
- Its chunking rule is the most realistic one I found: "word-like tokens, the way a tokenizer splits text before it encodes it: a word with the space before it, up to three digits of a number, a run of punctuation, or whitespace". `"Hello, world!"` becomes `Hello`, `,`, ` world`, `!`. `"Order 12345"` becomes `Order`, ` `, `123`, `45`. Chinese and Japanese text goes one character per token. Tool call arguments are chunked the same way.
- `thatStreams("Hel", "lo")` gives exact tokens, and `.andFails(new RuntimeException("connection reset"))` breaks the stream after them.
- Like a provider, it answers on its own thread, so the test has to wait for completion.
- The Javadoc warns that a mock "keeps every request it receives, so it is not meant for load tests or other long runs", and says to create a new one per test.

Sources:

- https://raw.githubusercontent.com/langchain4j/langchain4j/main/langchain4j-test/src/main/java/dev/langchain4j/test/mock/MockChatModel.java
- https://raw.githubusercontent.com/langchain4j/langchain4j/main/langchain4j-test/src/main/java/dev/langchain4j/test/mock/MockStreamingChatModel.java
- https://raw.githubusercontent.com/langchain4j/langchain4j/main/langchain4j-core/src/test/java/dev/langchain4j/model/chat/mock/ChatModelMock.java
- https://github.com/langchain4j/langchain4j/tree/main/langchain4j-test/src/main/java/dev/langchain4j/test/mock

## langchaingo - `llms/fake`

- Family: scripted list.
- `fake.NewFakeLLM(responses []string)`. Each call returns the next string. When the index reaches the end it resets to 0, so the list cycles.
- An empty list returns the error `no responses configured`.
- It checks the context first and returns the context error if it was cancelled.
- Streaming: if the caller set a streaming function, the fake calls it once with the whole response as bytes. An error from that callback aborts the request.
- `Reset()` puts the index back to 0 and `AddResponse(s)` appends.
- Tool calls, structured output, usage: the fake returns only text content.
- The package comment says it is "in the spirit of testing/fstest's MapFS".

Sources:

- https://raw.githubusercontent.com/tmc/langchaingo/main/llms/fake/fakellm.go

## Instructor

- The repo's contributor guide is blunt: "No mocking in tests - tests make real API calls". Tests are organised per provider, and the guide gives a pytest filter (`-k 'not llm and not openai'`) to skip the ones that need a model.
- The debugging docs describe one local example (`examples/debugging/run.py`) that "Patches a fake provider `create` with `instructor.patch(mode=Mode.JSON)`" and parses a canned JSON payload. So the seam is the provider client's `create` function, and the fake is a function you write.
- A third-party course page claims a built-in `instructor.mock()` helper. I did not find it in the official docs: not confirmed.

Sources:

- https://raw.githubusercontent.com/567-labs/instructor/main/CLAUDE.md
- https://python.useinstructor.com/debugging/

## LiteLLM - `mock_response`, `mock_tool_calls`, `mock_delay`, `mock_timeout`

These are per-call keyword arguments on `completion()` and `acompletion()`. When one is set, the call skips the provider and builds a response in the OpenAI shape.

- Family: fixed text per call, with magic values for failures.
- `mock_response="some text"`: the content of `choices[0].message`. The default text when the mock path runs without one is `This is a mock request`.
- `mock_response` as a dict: becomes `ModelResponse(**dict)`. As a `ModelResponse`: returned as is, or converted to a stream when `stream=True`.
- `mock_response` as an exception instance: raised (an `openai.APIError` as is, anything else wrapped in `MockException` with its `status_code`, default 500).
- Magic strings that raise typed errors: `"litellm.RateLimitError"`, `"litellm.ContextWindowExceededError"`, `"litellm.InternalServerError"`. A string starting with `Exception: content_filter_policy` raises a 400. A string starting with `Exception: mock_streaming_error` raises a 529 in the middle of the stream.
- `mock_tool_calls=[{...}]`: each dict is turned into a `ChatCompletionMessageToolCall` and placed on `choices[0].message.tool_calls`. The test writes the id, name and arguments by hand.
- `mock_delay=seconds`: sleeps before answering (blocking sleep for sync, `asyncio.sleep` for async).
- `mock_timeout=True` together with `timeout=...`: sleeps for the timeout, then raises `litellm.Timeout`.
- `n=3`: three choices, all with the same text.
- Streaming: the text is cut into chunks of exactly 3 characters (`mock_response[i : i + 3]`), each sent as a delta with role `assistant`. A final usage chunk follows when prompt tokens are known.
- Usage: prompt tokens default to 10 and completion tokens to 20. Both are constants that can be overridden through environment variables (`DEFAULT_MOCK_RESPONSE_PROMPT_TOKEN_COUNT`, `DEFAULT_MOCK_RESPONSE_COMPLETION_TOKEN_COUNT`).
- Metadata: `model` is the model the caller asked for, and the provider is inferred from that name. The docs page still shows an older sample with `"model": "MockResponse"` and null usage, which does not match the current source.
- Mock embeddings: `[0.0] * 1536` with prompt tokens 10. The string `"error"` raises.
- The mock path still runs LiteLLM's pre-call and post-call logging hooks (I saw both in `mock_completion`). Because the mock is an ordinary request parameter, it suits tests of the code around the model, such as fallbacks, retries and rate-limit handling. That last sentence is my reading, not a quote.
- What the docs say: "For testing purposes ... This will return a response object with a default response (works for streaming as well), without calling the LLM APIs." The page title adds "Save Testing Costs".

Sources:

- https://docs.litellm.ai/docs/completion/mock_requests
- https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/main.py
- https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/utils.py
- https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/constants.py
- https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/litellm_core_utils/mock_functions.py

## Ollama and llama.cpp

- Ollama: a feature request for a mocked model (issue #4196, "Mocked model for integration testing Ollama") is still open. An older request for a dummy model (#728) was closed with a maintainer's advice to "create a mock or stub that returns expected results" with an HTTP test server. Search excerpts only.
- Ollama's own Go unit tests swap the inference runner for a `mockRunner` struct with `CompletionFn` and `ChatFn` hooks and canned `CompletionResponse` values. That is test-only code inside the server package, so it is a user-function fake at the runner boundary.
- llama.cpp: the server test suite starts the real `llama-server` binary with a tiny story model (`stories260K`) and a fixed seed of 42. So its tests run real inference on a very small model. Search excerpts of the test README and `utils.py` only.
- A third-party project, `kinduff/ollama-mock`, describes a fake Ollama server where "each answer is seeded by a hash of everything that shapes it: the model, the messages, the system prompt, the tools, the format". Search excerpt only, and it is a server, so it belongs to the mock-server topic. I note it here because hash-seeded output is a distinct way to produce text.

Sources:

- https://github.com/ollama/ollama/blob/main/server/routes_generate_test.go
- https://github.com/ollama/ollama/issues/4196

## Rust crates

Two Rust crates ship a real mock model (Rig and Swiftide). The others test at the HTTP layer or replay recordings.

### Rig - `rig::test_utils::MockCompletionModel`

- Where: `rig::test_utils`, behind the `test-utils` cargo feature, meant for dev-dependencies. Public and documented.
- Family: strict scripted queue.
- Constructors: `MockCompletionModel::text("...")` (one text turn), `from_turns([MockTurn, ...])` for normal calls, `from_stream_turns([[MockStreamEvent, ...], ...])` for streamed calls. The two queues are separate: a normal call takes from the turn queue and a streamed call takes from the stream queue.
- The rule: "Each call consumes exactly one scripted turn. If no turn is available, the call fails with `ProviderError::Provider` and a clear message instead of repeating previous responses." The messages are `mock completion model has no scripted completion turn` and `mock completion model has no scripted streaming turn`. The docs give the reason: "so a test can't silently pass on a repeated answer".
- `MockTurn` builders: `text(...)`, `tool_call(id, name, json_args)`, `error(message)`, `request_error(message)`, `provider_response_error(status, body, request_id)`, `from_content(...)`, `from_contents(...)`, and modifiers `with_usage`, `with_response_id`, `with_provider_request_id`, `with_finish_reason`, `with_raw`, `with_call_id`.
- Turns derive `Serialize` and `Deserialize`. A source comment says "A turn is data: a script serializes, so a scripted model can be written to a fixture and read back", and `script()` and `stream_script()` return the turns that are still unconsumed.
- State is an `Arc` holding three mutex-protected parts: the turn queue, the stream-turn queue, and the request log. "Clones share the script and the recorded requests", so the test keeps one clone as a probe and gives the other to the agent.
- Recording: `requests()` returns the cloned `CompletionRequest` values, `request_count()` the number. The docs assert on `sent[0].system_instructions()`. A helper `sent_documents(&request)` lists the context documents an agent attached.
- Streaming: the test writes the events. `MockStreamEvent` variants: `Text`, `TextStart`, `TextAdditionalParams`, `ToolCall` (a whole call), `ToolCallNameDelta`, `ToolCallArgumentsDelta`, `ToolCallEnd` (a call sent as fragments), `Reasoning`, `ReasoningDelta`, `Unknown` (a provider item Rig does not model), `RequestId`, `FinalResponse` (holds the usage), `Error` (fails the stream at that point).
- So tool-argument fragmenting and mid-stream failure are both scriptable, and the events run through the same decoder path that real provider wires use.
- Tool calls: `MockTurn::tool_call("call_1", "add", json!({ "x": 2, "y": 3 }))` then `MockTurn::text("2 + 3 = 5")`. The crate ships a ready-made `MockAddTool`. Arguments are hand-written.
- Usage: `Usage::default()` unless the turn sets one. A turn reports a finish reason only when the test sets it.
- Identity: provider `mock`, model `mock-model`, API name `mock.script`. Request options such as `seed`, `top_p` and `stop` are accepted and ignored, with the stated reason "a scripted reply ignores options", so a test reads them back from the recorded request.
- `MockEmbeddings::model()` returns the same fixed vector for every text.
- What the docs say: "Keep mock-based tests about your own code: tool wiring, prompt construction, retrieval, and error handling. Whether a real model gives good answers is a question for recorded runs and manual review, not for scripted mocks." They point to a separate "Recording & Replay" page for replaying a real run in CI. I could not open that page (my guess at its URL returned 404).

Sources:

- https://rig.rs/docs/concepts/testing/
- https://docs.rs/rig-core/latest/src/rig_core/test_utils/completion.rs.html
- https://docs.rs/rig-core/latest/src/rig_core/test_utils/streaming.rs.html

### Swiftide - `MockChatCompletion` and the documented `ScriptedLlm` pattern

- The agent testing guide first shows a fake you write yourself, `ScriptedLlm`: a `VecDeque<ChatCompletionResponse>` and a request log, both behind `Arc<Mutex<..>>`. `complete()` logs the request's messages and pops the front reply. When the script runs out it returns a permanent `LanguageModelError` with the text "the script has no more replies".
- Only `complete` has to be implemented. The trait's default `complete_stream` wraps the single response in a one-item stream.
- A helper `reply_calling(name, json_args)` builds a response with one tool call whose id is `call_{name}`.
- The shipped mock is `swiftide_core::test_utils::MockChatCompletion`, behind the `test-utils` feature. It is an expectation mock, stricter than a script:
    - `llm.expect_complete(expected_request, Ok(response))` queues a pair. Expectations are consumed in the order they were added.
    - Each incoming request is compared for equality with the next expected request (messages plus the full set of tool specs). A mismatch panics and prints both requests and the remaining expectations.
    - When the last clone is dropped with expectations left, it panics. "A test that stops early fails even when every request it made matched."
    - An `Err` response reaches the agent as a permanent `LanguageModelError`.
- Streaming in the shipped mock: it checks the expectation, then a spawned task splits the message on whitespace and sends one growing response per word, sleeping 10 ms between words.
- The docs list real gotchas with that: on a response without text the task panics and the stream closes empty, so "a streamed reply that only calls tools never delivers its tool calls". The advice is to script a message on every reply when the agent streams.
- Another gotcha from the docs: the agent rewrites JSON object arguments before storing them, so expected requests must be built with `serde_json` and not as hand-typed strings.
- What the docs say: use the scripted model for end-to-end agent tests, and `MockChatCompletion` "for tests that pin down the exact requests".

Sources:

- https://swiftide.rs/agents/testing/
- https://docs.rs/swiftide-core/latest/src/swiftide_core/test_utils.rs.html

### genai (rust-genai) - recorded cassettes

- The `tests` folder has two groups: `tests_p_*.rs` files, one per live provider, and `tests_yakbak_*.rs` files.
- The yakbak tests are replay tests. The file header says: "These tests use pre-recorded cassettes from `tests/data/yakbak/openai/`". Each test calls `replay_client("openai", "stream_tool_call_with_finish_reason")`, which returns a client and a second value the tests bind as `_server`, and then runs the real adapter through that client. I did not read the `support::yakbak` helper itself.
- Fixtures use stable ids such as `call_yakbak_fixture_0`, and the tests assert on exact usage numbers from the recording (163 prompt tokens, 11 completion tokens).
- A file named `tests_yakbak_record.rs` sits next to them. I did not read it.
- A code search for "mock" in the repo returned zero hits. So this crate's answer to "how do tests avoid inference" is record and replay at the HTTP layer.

Sources:

- https://raw.githubusercontent.com/jeremychone/rust-genai/main/tests/tests_yakbak_openai.rs
- https://github.com/jeremychone/rust-genai/tree/main/tests

### async-openai - a tower service seam

- The `middleware` feature lets you replace the HTTP service: `Client::with_config(...).with_http_service(service)`.
- The docs: "You can replace `ReqwestService` entirely. This is useful for routing, logging, metrics, tests, mocks, etc.", and the sample closure has a comment saying you can "return a synthetic response for testing".
- So the fake is a `tower::service_fn` that receives an `HttpRequestFactory` and returns a `reqwest::Response`. It works at the HTTP level and in process, and the service decides whether anything goes on the network.
- A code search for "mock" in the repo matched only those two doc passages.

Sources:

- https://raw.githubusercontent.com/64bit/async-openai/main/async-openai/MIDDLEWARE.md

### langchain-rust, llm (graniet), aisdk, kalosm

- langchain-rust: its LLM tests are "Offline tests for the OpenAI client: assert on the JSON wire format and on responses from a local mock server", using the `mockito` crate (`src/llm/openai/mock_tests.rs`, with matching files for Claude and Bedrock). The `LLM` trait has two required methods, `generate` and `stream`, and its default `generate_stream` emits the `generate` result as a single chunk (search excerpt only), so a hand-written fake is short. A code search for "mock" found only HTTP mock servers and a `RetrieverMock` in an example.
- llm (graniet): a code search found `mockito` in the dev-dependencies and a `create_mock_response(chunks)` helper that feeds byte chunks to the SSE parser tests. Search excerpts only.
- aisdk (lazy-hq): the provider test macro starts with a `skip_if_no_api_key!` check, so those tests call live providers and skip when the key is missing. `wiremock` is in its dev-dependencies. Search excerpts only.
- kalosm: not confirmed. My code search of the `floneum` repo came back incomplete.
- A mock model type shipped by langchain-rust, llm or aisdk: not confirmed for any of the three.

Sources:

- https://raw.githubusercontent.com/Abraxas-365/langchain-rust/main/src/llm/openai/mock_tests.rs

## Families - sorted by how the fake decides what to say

### 1. Constant

- The fake says the same thing every time.
- Members: LangChain `FakeChatModel` ("fake response"), Mastra `createMockModel` and `MockProvider`, LiteLLM `mock_response="..."`, LlamaIndex `MockLLM(max_tokens=N)` (N copies of the word `text`), Haystack `MockChatGenerator(responses="...")`, Genkit `mockModel` with a single response, LangChain4j `thatResponds("...").thenRepeats()`.
- Best example: LiteLLM `mock_response`, because the constant rides on the normal request and comes with ready-made failure values (`"litellm.RateLimitError"`), a delay and a timeout. https://docs.litellm.ai/docs/completion/mock_requests
- Good for smoke tests and plumbing. It tells you nothing about multi-turn flow.

### 2. Scripted list that cycles or repeats

- Each call takes the next item. At the end the list wraps to the start, or the last item repeats.
- Wraps to the start: LangChain `FakeListLLM`, `FakeListChatModel`, `FakeMessagesListChatModel`, the private `FakeToolCallingModel`, Haystack `MockChatGenerator(responses=[...])`, langchaingo `fake.LLM`.
- Repeats the last item: Genkit `mockModel` with an array, Vercel `mockValues`, DSPy `DummyLM` (it keeps saying "No more responses").
- Best example: Haystack `MockChatGenerator`, since list items can be full messages with tool calls and it adds metadata and streaming on top. https://raw.githubusercontent.com/deepset-ai/haystack/main/haystack/components/generators/chat/mock.py
- The risk is the one Rig's docs name: a test can pass on a repeated answer without anyone noticing the extra model call.

### 3. Strict script (errors at the end, checks for leftovers)

- Each call consumes exactly one step. One call too many is an error. Leftover steps are an error too, through an explicit check or on drop.
- Members: OpenAI Agents `ScriptedModel` (Python and JS), Rig `MockCompletionModel`, LangChain4j `MockChatModel` and `MockStreamingChatModel`, AutoGen `ReplayChatCompletionClient`, LangChain `GenericFakeChatModel` (the iterator runs dry), Google ADK `MockModel` (index error), Swiftide's documented `ScriptedLlm`, Vercel `MockLanguageModelV4` with an array (returns `undefined` past the end, which is the weakest form).
- Leftover checks: OpenAI `assert_complete()`, Swiftide `MockChatCompletion` (panics on drop).
- Best example: OpenAI Agents SDK `ScriptedModel`, for the pairing of `UnexpectedModelCall` with `assert_complete()`, the call recording, and docs that spell out what the fake is for. https://openai.github.io/openai-agents-python/testing/
- Best Rust example: Rig `MockCompletionModel`. https://rig.rs/docs/concepts/testing/

### 4. Expectation mock (script plus exact request matching)

- Like a strict script, and each step also states the exact request it expects. A different request fails the test.
- Members: Swiftide `MockChatCompletion`. The OpenAI Agents sandbox fake uses the same idea with a per-step `match` function.
- Best example: Swiftide `MockChatCompletion`. https://docs.rs/swiftide-core/latest/src/swiftide_core/test_utils.rs.html
- Very tight. The Swiftide docs show the cost: expected requests have to list every default tool spec and match argument formatting exactly.

### 5. Echo

- The reply is built from the input.
- Members: LangChain `ParrotFakeChatModel` (returns the last message), LlamaIndex `MockLLM` without `max_tokens` (returns the prompt), Haystack `MockChatGenerator()` with zero config (text of the last message), Genkit `echoModel` (the whole rendered request plus config), LangChain's private `FakeToolCallingModel` (all message texts joined with `-`).
- Best example: Genkit `echoModel`, because it echoes the entire rendered conversation with role labels, which turns the reply into a prompt-assembly assertion, and it fails loudly when a schema is requested. https://genkit.dev/docs/testing/
- Good for checking what the model was sent, through the normal output path, with zero setup.

### 6. User function

- The test supplies a function from request to response.
- Members: Pydantic AI `FunctionModel`, Vercel `MockLanguageModelV4` (`doGenerate`, `doStream`), Genkit `mockModel` callback, Haystack `response_fn`, LangChain4j `thatResponds(Function)`, OpenAI `ModelStep.respond` and `modelResponder`, LlamaIndex `MockFunctionCallingLLM(response_generator=...)`, Microsoft.Extensions.AI `TestChatClient`, CrewAI `BaseLLM` subclass, Ollama's internal `mockRunner`, async-openai's custom tower service.
- Best example: Pydantic AI `FunctionModel`, because the function receives the tool and output schemas (`AgentInfo`) along with the messages, and the streaming variant yields text or tool-call fragments. https://ai.pydantic.dev/testing/
- The most flexible family. The cost is that each test has to write its own little model.

### 7. Lookup table keyed on the input

- The reply is chosen by matching the input against keys.
- Members: DSPy `DummyLM` with a dict (first key that is a substring of the last message) and with `follow_examples=True` (copies the matching demo out of the prompt).
- Best example: DSPy `DummyLM`. https://raw.githubusercontent.com/stanfordnlp/dspy/main/dspy/utils/dummies.py
- Order-independent, which suits concurrent calls where a queue would hand replies to the wrong caller. LangChain4j's docs make the same point for computed steps: use them "to answer concurrent requests by their content".

### 8. Schema-driven auto-pilot

- With zero scripting, the fake reads the tool and output schemas from the request, calls the tools with generated arguments, then answers.
- Members: Pydantic AI `TestModel` (full JSON Schema walker, seeded), LlamaIndex `MockFunctionCallingLLM` default generator (calls every tool, fills only the fields that have defaults).
- Best example: Pydantic AI `TestModel`. https://raw.githubusercontent.com/pydantic/pydantic-ai/main/pydantic_ai_slim/pydantic_ai/models/test.py
- This is the only family that exercises every registered tool and the output validator without the test author writing a single reply.

### 9. Record and replay

- Real provider traffic is recorded once and replayed in tests.
- Members: rust-genai's yakbak cassette tests. Rig's docs mention a recording and replay feature (page not read).
- Best example: rust-genai. https://raw.githubusercontent.com/jeremychone/rust-genai/main/tests/tests_yakbak_openai.rs
- This sits at the wire level, so it belongs with the mock-server topic more than with in-process fakes.

### Cross-cutting: how streaming chunks are made

- Hand-written chunks: Vercel (`simulateReadableStream`), Rig (`MockStreamEvent`), Pydantic AI `FunctionModel`, OpenAI `ModelStep.stream`, LangChain4j `thatStreams(...)`, Genkit `sendChunk`.
- Derived from the full reply, one chunk for everything: OpenAI `ScriptedModel` (one delta per text part), Mastra v2, langchaingo, Google ADK, Swiftide's default `complete_stream`.
- Derived, one character per chunk: LangChain `FakeListChatModel` and `FakeStreamingListLLM`, LlamaIndex `MockLLM` in echo mode.
- Derived, three characters per chunk: LiteLLM.
- Derived, word by word: Pydantic AI `TestModel` (split on spaces, a lone word cut in half), LangChain `GenericFakeChatModel` (words and whitespace as separate chunks), Haystack (word plus trailing whitespace), AutoGen (word plus a space), Mastra v1, Swiftide `MockChatCompletion` (with a 10 ms pause).
- Derived, tokenizer-like: LangChain4j `MockStreamingChatModel` (word with leading space, digits in groups of three, punctuation runs, one character for Chinese and Japanese), applied to tool-call arguments as well.
- Failure in the middle of a stream: LangChain `error_on_chunk_number`, LangChain4j `.andFails(...)`, LiteLLM `"Exception: mock_streaming_error"`, Rig `MockStreamEvent::Error`, Pydantic AI stream cancel.

### Cross-cutting: token usage

- All zeros: DSPy.
- Fixed numbers: LiteLLM (10 and 20), Mastra (10 and 20), OpenAI `ScriptedModel` (one request, zero tokens unless set).
- Estimated from the text: Pydantic AI (split on whitespace and punctuation, plus 50 input tokens of overhead, plus 1 per tool call), Haystack and AutoGen (whitespace word counts).
- Whatever the test writes: Vercel, Rig, Genkit, LangChain4j, Google ADK.

### Cross-cutting: guard rails

- A global switch that blocks real model requests: Pydantic AI `ALLOW_MODEL_REQUESTS = False`. It is the only one I found.
- Swapping the model without touching call sites: Pydantic AI `Agent.override(model=...)`, LlamaIndex `Settings.llm = MockLLM(...)`, Genkit registering the mock under the app's default model name.
- Separate side channels to switch off: OpenAI Agents tracing (`tracing_disabled`, `OPENAI_AGENTS_DISABLE_TRACING=1`).
- Skipping live tests when the key is missing: aisdk's `skip_if_no_api_key!`, Instructor's pytest filter.

## Design ideas worth copying for a Rust in-process fake

Each idea has a confidence level for "this is worth building".

1. Make the script strict by default, with an explicit opt-in to repeat. One call consumes one step, an extra call returns a clear error that prints the request, and `assert_complete()` (or a drop check) catches leftovers. Add a `then_repeats()` that refuses to repeat a tool-call step, as LangChain4j does. Confidence: high - OpenAI Agents, Rig, LangChain4j and Swiftide all converged on this, and Rig's docs state the reason.
2. Add a schema-driven auto-pilot as the zero-config mode, modelled on `TestModel`. Turn 1: one tool call per offered tool, with arguments from a seeded JSON Schema walker (const, enum by seed, `$ref`, `anyOf` by seed, required properties only, min-bound strings, numbers and arrays, canned values per `format`). Turn 2: the final text is a JSON map of tool name to tool result, or schema-generated JSON when the request has a `response_format` schema. This replaces the "always pong" backend with something that exercises every tool path in the harness. Confidence: high - it is the one design here that needs zero per-test scripting, and the algorithm is small and fully deterministic.
3. Layer the modes in one type, the way Haystack and Genkit do: scripted steps first, a per-step or global responder closure for request-dependent replies, auto-pilot or echo as the default when nothing is scripted. Echo the whole rendered request (Genkit style) so the reply doubles as a prompt-assembly assertion. Confidence: medium - convenient, and the precedence rules between modes need care so a test never gets a surprising fallback.
4. Derive streaming from the complete reply with a deterministic tokenizer, and allow exact overrides. Use LangChain4j's word-like rule for text and for tool-call argument JSON, emit OpenAI-shaped fragments (name first, then argument pieces, as Rig's `ToolCallNameDelta` and `ToolCallArgumentsDelta` model), and offer `streams(["Hel", "lo"])` plus `and_fails(...)` for exact chunks and a break in the middle. Since the system under test speaks SSE, argument fragmenting is where real bugs live, and most fakes here send arguments in one piece. Confidence: high - the single-chunk fakes (OpenAI auto stream, Mastra v2, langchaingo) would miss reassembly bugs, and Swiftide's docs record a real defect in its word-splitting stream for tool-only replies.
5. Record every request and make all output metadata deterministic. Keep snapshot copies of requests behind `Arc<Mutex<..>>` so clones share them (Rig), use predictable ids (`tool_call_1`, or `call_{tool_name}`, never a random UUID as LlamaIndex does), estimate usage from the text (word and punctuation count plus a flat overhead, as Pydantic AI does) so usage-accounting code sees plausible, stable numbers, and make steps `serde` serialisable so scripts can live in fixture files (Rig). Confidence: high - cheap to build and every mature fake here has some form of it.

Two smaller ideas:

- Failure steps as first-class script items: an error with an HTTP status and request id (Rig `provider_response_error`), named failures such as rate limit and context overflow (LiteLLM's magic strings), a delay, and a timeout. Confidence: medium - valuable for the gateway's retry and fallback paths, and the set of failures to model depends on what the gateway handles.
- A process-wide switch that makes every real provider client refuse to send during tests, as Pydantic AI's `ALLOW_MODEL_REQUESTS` does. Confidence: medium - one guard is a small amount of code, and its value depends on how many code paths can construct a real client.

## What I could not confirm

- LlamaIndex: the exact text `MockLLM` echoes for chat calls (that goes through the `CustomLLM` base class, which I did not read).
- DSPy: how `DummyLM` chunks a streamed reply (the `response_to_events` helper), and whether any path emits tool calls.
- LangChain: whether `GenericFakeChatModel` streams the `tool_calls` field as tool-call chunks. The `_stream` method I read handles only `content` and `additional_kwargs`.
- Vercel AI SDK: the contents of `decision-mock-model-v4.ts`, `extraction-mock-model-v4.ts` and `mock-provider-v4.ts`, and of `MockLanguageModelV2` (seen in the folder listing only). `MockLanguageModelV1` is absent from the current main folder and I did not open the older release line.
- AutoGen: the tool-call shape inside `CreateResult`.
- Semantic Kernel: a reusable fake chat service shipped in a package. I found a team blog post that uses Moq, and private mock classes in samples (search excerpts).
- Microsoft.Extensions.AI: a fake client in the shipped packages. `TestChatClient` lives in the test project.
- Google ADK: a mock model in the published package. `MockModel` lives in the test tree. The in-repo testing reference was a search excerpt only.
- CrewAI: a fake LLM class in the package, and the `EchoLLM` docs section (pull request seen as a search excerpt only).
- Spring AI: a fake chat model from the project itself. The reference testing page covers evaluators with real models.
- Instructor: the `instructor.mock()` helper that a third-party course page describes.
- Haystack: `MockTextEmbedder` and `MockDocumentEmbedder` behaviour (release note excerpt only).
- Mastra: `MastraLanguageModelV2Mock` internals.
- Ollama and llama.cpp: both findings rest on search excerpts plus the Ollama test file. The llama.cpp test README and `utils.py` reached me through the search tool, and I did not fetch them directly.
- Rig: the "Recording & Replay" page.
- rust-genai: the `support::yakbak` helper and `tests_yakbak_record.rs`.
- llm (graniet), aisdk: search excerpts only. kalosm: nothing found, search incomplete.
- LangChain4j: `MockEmbeddingModel`, `MockModerationModel`, `MockScoringModel`, `MockDecisionModel` (file names only).
- LiteLLM: the docs sample response (`"model": "MockResponse"`, null usage) disagrees with the current source (model name from the request, usage 10 and 20). I trust the source.
