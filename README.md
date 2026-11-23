[![CI](https://github.com/cppalliance/cheapseek/actions/workflows/ci.yml/badge.svg)](https://github.com/cppalliance/cheapseek/actions/workflows/ci.yml)
[![Linux](https://img.shields.io/github/check-runs/cppalliance/cheapseek/master?label=Linux&nameFilter=test%20%28ubuntu-latest%29)](https://github.com/cppalliance/cheapseek/actions/workflows/ci.yml)
[![Windows](https://img.shields.io/github/check-runs/cppalliance/cheapseek/master?label=Windows&nameFilter=test%20%28windows-latest%29)](https://github.com/cppalliance/cheapseek/actions/workflows/ci.yml)
[![macOS](https://img.shields.io/github/check-runs/cppalliance/cheapseek/master?label=macOS&nameFilter=test%20%28macos-latest%29)](https://github.com/cppalliance/cheapseek/actions/workflows/ci.yml)

# CheapSeek

CheapSeek is a test server that speaks the OpenAI chat-completions API and answers by rule, with zero inference. It is one small executable. Point the tests of a gateway or an agent harness at it in place of a real model, and they run fast, on the local machine, with the same reply to the same request every time.

![The CheapSeek mascot: a robot built from cardboard and packing tape, with bottle caps for eyes, denim patches, and its name written on its chest, in a garage workshop with an original Macintosh on the bench](images/cheapseek-mascot.jpg)

- **It talks back.** Plain chat is answered by [RustBot](https://github.com/Shekswess/rustbot), a Rust port of Hugging Face's rule-based [PyBot](https://github.com/huggingface/pybot). A reply depends on what was said, and on what was said earlier in the conversation.
- **It calls tools.** A test writes the tool calls it wants into the prompt, or lets the request ask for one, and the server answers with real `tool_calls`.
- **It is strict OpenAI.** The routes, the fields, the stream and the error envelope are OpenAI's, and the server adds nothing of its own. A test steers it with the model name and the prompt text.
- **It keeps nothing that changes a reply.** Every reply is computed from the request alone, so one server can answer any number of tests at once, in any order.

## Run

Get the executable in one of three ways:

- **Download it.** Each [release](https://github.com/cppalliance/cheapseek/releases) holds one file for Linux on x64, one for macOS on arm64 and one for Windows on x64, with their SHA-256 sums. The Linux file needs glibc 2.35 or newer, which Ubuntu 22.04 has. On Linux and macOS, make the file executable with `chmod +x`.
- **Install it with cargo.** `cargo install --git https://github.com/cppalliance/cheapseek --locked` builds the executable in about three minutes and puts `cheapseek` among cargo's own executables. It needs Rust 1.99 or newer.
- **Build it in a checkout.** `cargo build --release` leaves it at `target/release/cheapseek`.

Then start it:

```sh
cheapseek
```

The server binds a free port on `127.0.0.1`, warms the bot, prints one line, and serves until Ctrl-C:

```text
http://127.0.0.1:54321/v1
```

That line is the base URL an OpenAI client takes. `--port N` picks the port. The server answers any caller on the local machine, and an `Authorization` header is accepted and ignored.

Starting takes about two seconds, because the bot compiles most of its rules on its first turn. The process uses 300 to 400 MiB of memory.

## Use it in GitHub Actions

One step starts a server that runs until the job ends:

```yaml
- uses: cppalliance/cheapseek@v1
  id: cheapseek
- run: cargo test
  env:
    OPENAI_BASE_URL: ${{ steps.cheapseek.outputs.base-url }}
    OPENAI_API_KEY: unused
```

The step downloads the executable of its release, compares it with the SHA-256 sum of that release, starts it, and waits for the base URL.

- **`base-url`** is the output: the base URL, such as `http://127.0.0.1:54321/v1`. Later steps of the job also find it in the environment variable `CHEAPSEEK_BASE_URL`.
- **`port`** is an input: the port to listen on. The default, `0`, takes a free port.
- **`executable`** is an input: the path of a CheapSeek executable to start in place of the download, such as one the job built.
- **Runners.** A release holds executables for Linux on x64, macOS on arm64 and Windows on x64, the systems of GitHub's `ubuntu-latest`, `macos-latest` and `windows-latest`. On another runner, build CheapSeek in the job and pass its path as `executable`.
- **Versions.** `@v1` follows the newest 1.x release, and a release can change a reply. `@v1.0.0` or a commit hash keeps the replies fixed.

## Point a client at it

Any OpenAI client works with the printed base URL and any API key.

```sh
curl http://127.0.0.1:54321/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"cheapseek","messages":[{"role":"user","content":"Hi, I'\''m Sam."}]}'
```

```json
{
  "id": "chatcmpl-541fd0221761ddac",
  "object": "chat.completion",
  "created": 1795305600,
  "model": "cheapseek",
  "choices": [
    {
      "index": 0,
      "message": { "role": "assistant", "content": "Nice to meet you, Sam. What can I help with?", "refusal": null },
      "logprobs": null,
      "finish_reason": "stop"
    }
  ],
  "usage": { "prompt_tokens": 9, "completion_tokens": 13, "total_tokens": 22 },
  "system_fingerprint": "fp_cheapseek"
}
```

**The Runvil Gateway** reaches it as a remote OpenAI endpoint. An entry of this shape follows the Gateway's own example config. It is untested here: the tests of this repository talk to the server directly.

```toml
[[endpoint]]
id = "cheapseek"
protocol = "openai"
base_url = "http://127.0.0.1:54321/v1"
api_key = ""

[[model]]
name = "cheapseek"
description = "A rule-based stand-in for tests"
context = 8192
upstream = "cheapseek"
endpoints = ["cheapseek"]
```

**A Rust test** can skip the process and run the server on a listener of its own:

```rust
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
let base_url = format!("http://{}/v1", listener.local_addr()?);
cheapseek::warm_up().await;
tokio::spawn(cheapseek::serve(listener, std::future::pending()));
```

## The two models

The model name picks how the reply is made.

| Model | What it answers |
|---|---|
| `cheapseek` | The first reply rule that fits the request. See the next section. |
| `cheapseek-echo` | The prompt as the server received it, one entry to a line. |

Any other name gets a 404 with the code `model_not_found`, so a misspelled name fails loudly.

The echo model shows a test what arrived: what a gateway added to the prompt, which tools a harness offered, and where a tool result went.

```text
tools: [{"type":"function","function":{"name":"get_weather","parameters":{"type":"object"}}}]
tool_choice: "auto"
system: Be brief.
user: Weather in Oslo?
assistant: [call get_weather {"city":"Oslo"}]
tool get_weather: 4 C and rain
```

## How `cheapseek` picks a reply

The first rule that fits wins.

1. **Script step.** The last user message holds directives, and one of them is for this turn. The model says or calls what the directive tells it.
2. **Tool results.** The request ends with tool results. The model quotes each one as `name: content`, so every tool loop ends.
3. **Tool asked for.** `tool_choice` names a function or says `required`, or the last user message holds a tool's name as whole words. The model makes one call, with arguments made from the tool's schema. `tool_choice: "none"` skips this rule.
4. **JSON wanted.** `response_format` asks for `json_schema` or `json_object`. The model writes JSON in that form.
5. **Chat.** The bot answers the user turns of the request.

The stop sequences and the token limit then shorten the reply.

### Directives

A directive is a block in double square brackets in a user message. Each block is one turn of the model.

| Directive | What the model does in that turn |
|---|---|
| `[[say TEXT]]` | Replies with exactly TEXT. |
| `[[call NAME {"a":1}]]` | Calls the tool NAME with these arguments. |
| `[[call NAME]]` | Calls the tool NAME with arguments made from its schema. |
| `[[call A {..}; call B {..}]]` | Makes both calls in one turn. |

- **The turn is counted from the request.** It is the number of assistant messages after the last user message. A message with three blocks is a script of three turns, and the whole script lives in the request.
- **Only `[[say` and `[[call` open a block,** each followed by whitespace. Any other text in double brackets, such as the nested array `[[1,2],[3,4]]`, is ordinary text.
- **A block that opens and cannot be read gets a 400** with the code `invalid_directive`.
- **The bot reads the user turns without their blocks.**
- **The text of a `say` block ends at the first `]]`.**

### A tool loop, start to end

The user message asks for one call. The first reply makes it.

```json
{
  "model": "cheapseek",
  "tools": [{ "type": "function", "function": { "name": "get_weather", "parameters": { "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] } } }],
  "messages": [
    { "role": "user", "content": "What is the weather in Oslo? [[call get_weather {\"city\":\"Oslo\"}]]" }
  ]
}
```

```json
"message": {
  "role": "assistant",
  "content": null,
  "tool_calls": [{ "id": "call_fd6ea0e36809271a", "type": "function", "function": { "name": "get_weather", "arguments": "{\"city\":\"Oslo\"}" } }],
  "refusal": null
},
"finish_reason": "tool_calls"
```

The harness runs the tool and sends the result back with the same user message. The script has one block and the model has taken one turn, so rule 2 answers: it quotes the result, and the loop ends.

```json
{ "role": "assistant", "content": null, "tool_calls": [{ "id": "call_fd6ea0e36809271a", "type": "function", "function": { "name": "get_weather", "arguments": "{\"city\":\"Oslo\"}" } }] },
{ "role": "tool", "tool_call_id": "call_fd6ea0e36809271a", "content": "4 C and rain" }
```

```json
"message": { "role": "assistant", "content": "get_weather: 4 C and rain", "refusal": null },
"finish_reason": "stop"
```

A longer script runs the same way: `[[call read_file {"path":"a.txt"}]] [[call get_weather]] [[say All done.]]` makes a call in turn one, a second call in turn two, and says `All done.` in turn three.

### A tool by its name

With tools on offer and no directive, the model calls a tool when the last user message holds its name as whole words. `get_weather`, `get weather` and `getWeather` are the same two words, so "Please get weather for Oslo." calls `get_weather`. "Forget weathering the storm" calls nothing. When the text names several tools, the name with the most words wins.

The arguments are made from the tool's JSON Schema, so they validate and say nothing about the request: the call above gets `{"city":"a"}`. A test that needs exact arguments writes a directive.

## Tokens

One rule cuts text into tokens, and every count in a reply follows from it. A test can work the numbers out by hand.

- A run of letters and digits is cut into tokens of four characters. The last token of a run holds what is left.
- Every other visible character is one token.
- Whitespace belongs to the token that follows it.

So `Hello, world!` is six tokens: `Hell`, `o`, `,`, ` worl`, `d` and `!`.

- **`usage.prompt_tokens`** counts the prompt as the echo model prints it, tools included.
- **`usage.completion_tokens`** counts the reply. A tool call counts the tokens of its name and of its arguments.
- **`max_tokens` and `max_completion_tokens`** cut the reply after that many tokens, with `finish_reason: "length"`. The arguments of a tool call are cut the same way, in the middle of their JSON, as a real model cuts them.
- **`stop`** ends the reply where the first stop sequence begins.
- **A stream sends one token per delta,** so the number of content deltas is `completion_tokens`.

## Streaming

With `"stream": true` the reply is a stream of server-sent events, in the order OpenAI sends them: a role chunk, one chunk per token, a finish chunk, and `data: [DONE]`. With `"stream_options": {"include_usage": true}` a last chunk with empty `choices` holds the usage, and every chunk before it has `"usage": null`.

```text
data: {"id":"chatcmpl-3a5538100006a55d","object":"chat.completion.chunk","created":1795305600,"model":"cheapseek","system_fingerprint":"fp_cheapseek","choices":[{"index":0,"delta":{"role":"assistant","content":"","refusal":null},"logprobs":null,"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-3a5538100006a55d", ... "choices":[{"index":0,"delta":{"content":"Nice"},"logprobs":null,"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-3a5538100006a55d", ... "choices":[{"index":0,"delta":{},"logprobs":null,"finish_reason":"stop"}],"usage":null}

data: {"id":"chatcmpl-3a5538100006a55d", ... "choices":[],"usage":{"prompt_tokens":9,"completion_tokens":13,"total_tokens":22}}

data: [DONE]
```

A tool call streams its id and its name in one chunk, then its arguments one token per chunk. The whole reply is ready before the first event leaves, so the events arrive without a pause between them.

## Structured output

- **`{"type": "json_schema", ...}`** gets a value made from the schema. The recipe reads `const`, `enum`, `examples`, `allOf`, `$ref`, `anyOf`, `oneOf` and `type`. An object gets its required properties only, a string its shortest text or a fixed value for its `format`, a number its lowest allowed value, and an array its shortest list. The schema `{"name": string, "age": integer >= 18}` gives `{"age":18,"name":"a"}`.
- **`{"type": "json_object"}`** gets the bot's reply inside an object: `{"reply": "..."}`.
- **The request's `seed`** picks among the choices a schema leaves open. Seed 1 gives `{"age":19,"name":"b"}` above, and takes the second value of an `enum`.
- **The recipe reads those keywords alone.** A string with a `pattern`, and a `oneOf` whose branches overlap, can get a value that does not validate.

## Errors

A request the server refuses gets a status in the 400s and OpenAI's error envelope.

```json
{ "error": { "message": "The model `gpt-4o` does not exist.", "type": "invalid_request_error", "param": "model", "code": "model_not_found" } }
```

| Request | Status |
|---|---|
| A body that is not a JSON object, a missing `model` or `messages`, a field of the wrong shape | 400 |
| `n` above 1, a token limit of 0, more than four stop sequences | 400 |
| `stream_options` without `stream`, `tool_choice` without `tools` | 400 |
| A tool message that answers no earlier tool call, a tool call without its tool message | 400 |
| A directive that cannot be read | 400 |
| A model name the server lacks | 404 |
| A path outside the API | 404 |
| The wrong method on a path of the API | 405 |
| A body larger than 64 MiB | 413 |

Every other field of the OpenAI API, such as `temperature`, is accepted and ignored.

## What is repeatable

The same request body gets the same reply bytes: from one process or two, in one test run or the next, today or next year, on Windows, Linux and macOS. CI checks recorded replies on all three, on every push and once a week.

- **Ids are hashes of the request body.** The completion id and each tool call id change when the request changes, and only then.
- **`created` is a constant,** `1795305600`.
- **`seed` is the one source of variation.** It picks the bot's wording where a rule has several, and the choices in generated JSON. A request without a seed uses seed 0.
- **Line endings do not matter.** Each user turn is collapsed to one line before the bot reads it.
- **The bot's clock stands still.** For the bot it is always the `created` time: 22 November 2026, midnight UTC. "When is the next full moon?" gets "Tuesday 24 November, in 2 days" on every day of every year. The clock and the time zone of the host change nothing.
- **Time zones come from inside the executable.** "What is the time difference in New York?" gets the same reply on a host with old zone files, new ones, or none.

One thing outside the request can still change a chat reply: the system's math library. A few of the bot's sums, such as loan payoffs and distances, can differ in their last digit between systems.

Tests that ask about other things get the same bytes everywhere. Tool calls, tool results, echo replies and generated JSON depend on the request alone.

A reply can change between versions of CheapSeek, because the rules and the bot can change. Pin a release or a commit where the exact words matter.

## What to expect of it

- **The bot reads the user turns alone.** The system prompt is echoed and counted, and the chat reply stays the same with it or without it. The bot works out its own side of the conversation again from the user turns, so it answers the same whatever the earlier assistant messages say.
- **The bot knows everyday topics.** It answers cooking, travel and household questions, and it remembers names and facts from earlier turns. A request outside those topics, such as one for code, gets a short general line.
- **The bot reads the first 500 characters of each user turn.** Its time per turn grows with the length of the line.
- **A tool is called only when the request asks for one.** A harness under test runs real tools, so the trigger stays in the test's hands.
- **Tool results end a loop even under `tool_choice: "required"`.** A real model would call again.
- **Each request gets one choice.**
- **The API is the model list and chat completions.** Any other route gets a 404.

## Speed

Measured on one Windows machine with a release build. Each request went through `curl`, a new process and a new connection every time.

| Request | Time |
|---|---|
| Start to the printed URL | about 2 s |
| A tool call, tool results, an echo, or JSON from a schema | about 1.4 ms |
| A request the server answered before | about 1.4 ms |
| A new conversation of two user turns | about 20 ms |
| The first question on a topic the process has not seen | up to 0.6 s |

The server keeps the bots of the last 64 conversations. A request that continues one of them runs the new turns alone, and gets the reply a full replay gives.

A cold build takes about two and a half minutes, nearly all of it RustBot's 370,000 lines of rules. A cold CI test job takes four to eight minutes.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo test --locked
cargo doc --no-deps --locked
cargo deny check
```

The tests talk to a real server over HTTP. Two crates this project does not write act as judges: `async-openai`, a typed OpenAI client, reads every kind of reply, and `jsonschema` validates every value made from a schema.

CI runs these checks on every push, with the tests on Linux, macOS and Windows. On each of the three it also starts a server through the action and compares its replies with recorded ones, byte for byte. A coverage job counts the lines the tests run with `cargo llvm-cov`, and fails below a floor of 99 percent. The release workflow holds each executable to the same recorded replies before it publishes, and then starts the published files through the action.

## Design

- [design/state-of-practice.md](design/state-of-practice.md) reports what other fake LLMs do and what CheapSeek adopts.
- The `research` folder holds the evidence behind the report.
- The `plan` folder holds the build plans and what each build found.

## License and notices

CheapSeek is under the Boost Software License 1.0. See [LICENSE](LICENSE).

A CheapSeek executable includes RustBot, which is under the Apache License 2.0:

- [RustBot](https://github.com/Shekswess/rustbot), a native Rust port of PyBot's rule-based conversation engine. CheapSeek builds it from the [C++ Alliance fork](https://github.com/cppalliance/rustbot) at commit `866aac5`. The fork changes two things in the original, and its README lists them: a caller can set the time that the bot reads, and time zones come from inside the executable.
- [PyBot](https://github.com/huggingface/pybot), by Hugging Face, the chatbot that RustBot ports. Apache License 2.0.
