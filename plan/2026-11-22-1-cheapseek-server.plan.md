---
name: CheapSeek Test Server
overview: "Build CheapSeek in `cheapseek/` as one Rust executable: an Axum server with a strict OpenAI chat-completions API, RustBot for chat text, and rule-driven tool calls. A build-and-timing check of the local RustBot checkout comes first, then eleven commits on `master`, each passing the whole verification list, with CI on Windows, Linux and macOS."
todos:
  - id: local-gate
    content: "Before commit 0: build the local RustBot checkout in release mode into a scratch target folder, run its own tests, time its CLI at three message lengths, compare two runs of one conversation, apply the stop rules"
    status: completed
  - id: plan-copy
    content: "Commit 0: copy the approved plan alone to cheapseek/plan/2026-11-22-1-cheapseek-server.plan.md and commit it on master"
    status: in_progress
  - id: research-report
    content: "Commit 1: move the six research files into cheapseek/research, add design/state-of-practice.md, check the references, commit"
    status: pending
  - id: scaffold-ci
    content: "Commit 2: package, lint, deny and CI files, AGENTS.md, short README, a server that prints its URL and serves the model routes and the 404 envelope, tests, verify, commit"
    status: pending
  - id: rustbot-gate
    content: "Commit 3: pin RustBot, write brain.rs with the one-line cut, the panic guard, deep stacks and warm start, recorded-reply tests, measure build and reply times, verify, commit, push 1, watch CI on three systems, apply the gate"
    status: pending
  - id: chat-json
    content: "Commit 4: chat completions as JSON with the chat rule, the echo model, the token rule, limits, seed and errors, tests including async-openai, verify, commit"
    status: pending
  - id: streaming
    content: "Commit 5: SSE streaming with the role chunk, token deltas, finish chunk, usage chunk on request and [DONE], tests, verify, commit"
    status: pending
  - id: schema-json
    content: "Commit 6: schema.rs generator and response_format support, tests judged by the jsonschema crate, verify, commit"
    status: pending
  - id: tool-calls
    content: "Commit 7: directive.rs and ladder rules 1 to 3, tool calls in JSON and in the stream, the pairing check, tests including a full tool loop through async-openai, verify, commit"
    status: pending
  - id: bot-cache
    content: "Commit 8: keep up to 64 finished bots for growing conversations, tests, verify, commit"
    status: pending
  - id: readme
    content: "Commit 9: full README.md and final AGENTS.md, verify, commit, push 2, watch CI to green"
    status: pending
  - id: record
    content: "Commit 10: add What the build found and the todo statuses to the plan copy, commit, push 3"
    status: pending
isProject: false
---

# CheapSeek: an OpenAI-flavored test server on RustBot

CheapSeek becomes one Rust executable in `cheapseek/`: an Axum server that speaks strict OpenAI chat completions, answers plain chat with RustBot, and makes tool calls by rule. The design follows section 10 of [cheapseek/design/state-of-practice.md](cheapseek/design/state-of-practice.md). Every commit goes on `master` and passes the whole verification list by itself.

## Where this starts

- `cheapseek` is on `master` at `565b32b`, clean, with `origin` set to the public `cppalliance/cheapseek`. It holds `LICENSE` (BSL-1.0), a two-line `README.md` and an empty `research/`.
- The report is written and uncommitted at `design/state-of-practice.md`. The six research files, named `2026-11-22-conduct-research-fake-llm-*.md`, wait in the workspace's research staging folder.
- RustBot is checked out beside this repo at `RustBot/`, at `fb89123`, the commit this plan pins. Its source was read there, and it is still to be compiled. Local tools: rustc 1.99.1, cargo-deny 0.20.2, 192 logical cores.

## What gets built

- **One package at the repo root**, a library with a thin `main`, shaped like `vibelight`. Edition 2024, toolchain pinned to `1.99` like the Gateway, the Gateway's lint tables, `publish = false`.
- **Source files** in `cheapseek/src/`: `main.rs` (arguments, runtime, bind, print), `lib.rs` (`serve`, `router`), `api.rs` (handlers), `wire.rs` (request and response types), `reply.rs` (the ladder), `directive.rs`, `schema.rs`, `tokens.rs`, `sse.rs`, `brain.rs` (the RustBot wrapper).
- **Tests**: unit tests in `foo-tests.rs` beside each module, and public behavior in one binary at `tests/it/main.rs` with topic files. Test names are sentences.
- **The API**: `GET /v1/models`, `GET /v1/models/{id}` and `POST /v1/chat/completions`, as JSON and as SSE. Every other path gets a 404 in OpenAI's error envelope. An `Authorization` header and unknown request fields are accepted and ignored.
- **The process**: `cheapseek [--port N]` binds `127.0.0.1` (a free port by default), warms the bot, prints one line such as `http://127.0.0.1:54321/v1`, and serves until Ctrl-C.
- **Models**: `cheapseek` and `cheapseek-echo`. Any other name gets a 404 `model_not_found`.

### How `cheapseek` picks a reply

The first rule that fits wins:

1. **Script step.** Blocks in the last user message that start with `[[call ` or `[[say ` are a script, one block per model turn. The turn is the count of assistant messages after that user message. `[[call read_file {"path":"a.txt"}]]` calls a tool, `[[call a {..}; call b {..}]]` makes two calls in one turn, and `[[say Done.]]` replies with that exact text. Arguments left out are generated from the tool's schema. A block that starts with one of the two verbs and fails to parse gets a 400. Any other `[[` text is ordinary text.
2. **Tool results.** When the messages after the last assistant message are tool results, the reply quotes each as `name: content`, so every loop ends.
3. **Tool asked for.** `tool_choice` names a function or says `required`, or the user text holds a tool's name as whole words (`get_weather`, `get weather`, `getWeather`). The reply is one call with schema-made arguments. `tool_choice: "none"` skips this rule.
4. **JSON wanted.** A `response_format` of `json_schema` gets a value generated from the schema. `json_object` gets the chat reply wrapped as `{"reply": "..."}`.
5. **Chat.** The user turns, with script blocks stripped and each collapsed to one line, are replayed through a fresh RustBot seeded by the request's `seed` field (0 by default).

`cheapseek-echo` skips the ladder and returns the prompt as received, one line per message.

Shared by every reply: Rustybin's token rule (four letters or digits to a token, one token for any other visible character) drives `usage`, the `max_tokens` cut with `finish_reason: "length"`, and one token per stream delta. `created` is a fixed constant, and ids come from a hash of the request body.

## The commits

```mermaid
flowchart LR
    LocalGate[Local gate] --> PlanCopy[Plan copy]
    PlanCopy --> Docs[Report commit]
    Docs --> Scaffold[Scaffold and CI]
    Scaffold --> Brain[RustBot gate]
    Brain --> PushOne[Push 1]
    PushOne --> Chat[Chat JSON]
    Chat --> Stream[Streaming]
    Stream --> Schema[Schema JSON]
    Schema --> Tools[Tool calls]
    Tools --> Cache[Bot cache]
    Cache --> Readme[README]
    Readme --> PushTwo[Push 2]
    PushTwo --> Record[Record]
```

- **Before commit 0 - the local gate.** Build the local `RustBot/` checkout in release mode with `--locked` and a scratch target folder, so the checkout stays clean. Record the build time and the executable size. Run its own tests and record the pass and fail counts. Pipe lines into its CLI at seed 0 with `-v` and record the time of the first reply, of warm short replies, and of lines of 500 and 2,000 characters. Run one four-line conversation in two processes and compare the output. The cut in commit 3 is 2,000 characters, or 500 if a 2,000 character line takes over 150 ms.
    - **Stop rules.** Stop and report if the build fails, if more than one of its own tests in ten fails, if the two runs differ, or if a warm short reply takes over 200 ms.
- **Commit 0 - the plan.** This plan alone, copied to [cheapseek/plan/2026-11-22-1-cheapseek-server.plan.md](cheapseek/plan/2026-11-22-1-cheapseek-server.plan.md).
- **Commit 1 - research and report.** Move the six research files into `cheapseek/research/` with their frontmatter as it is, and add `design/state-of-practice.md`. Check: the report's references to `research` resolve.
- **Commit 2 - scaffold and CI.** `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `.cargo/config.toml`, `deny.toml`, `.gitignore`, `.gitattributes`, `AGENTS.md`, a short `README.md`, and `.github/workflows/ci.yml`. The server binds, prints its URL, and serves the two model routes and the 404 envelope. CI copies the Gateway's pinned actions: `fmt`, `clippy`, `test` on Ubuntu, Windows and macOS with `cargo test --locked`, `docs`, `supply-chain`, and a `ci-green` gate job. Tests: the model list has both models in OpenAI's shape; an unknown path gets the 404 envelope; the built executable prints one URL line and answers on it.
- **Commit 3 - RustBot and the CI gate.** Add `rustbot` as a git dependency pinned to `fb89123ebde58ab12eca349fbf902dd640f5c3bf`, and allow that source in `deny.toml`. In the dev profile, dependencies build at `opt-level = 2` and `rustbot` builds with `debug = false`. Every profile keeps unwinding panics, because the guard depends on them. `brain.rs` exposes one function, turns in and reply out. Each user turn is collapsed to one line and cut to the length the local gate chose. A fresh `Bot::new(Some(seed))` takes one `turn` per user turn, and the last reply goes through `rustbot::one_line`. A `catch_unwind` turns a panic into `Hello!` and drops that bot, and the panic message goes to stderr. The runtime gets 8 MiB thread stacks, and bot work runs under `spawn_blocking`. One throwaway turn runs before the URL prints. Tests: a four-turn conversation matches recorded replies word for word, with the name told in turn one coming back in turn four, and it stays clear of dates and sums; twenty fresh bots agree; text that differs only in line endings gets the same reply; a `Bot` is `Send`, checked at compile time; a panic yields `Hello!`. Measure and record CheapSeek's cold dev and release build times, its executable size, the first reply on a fresh topic and the warm reply. Then **push 1** and watch CI.
    - **The gate.** Stop and report if RustBot fails to build on any of the three systems, if the recorded replies differ between runs or systems, if a warm reply takes over 200 ms in the test profile here, or if the cold CI test job runs past 45 minutes. rsbot is the named alternative, behind the same function.
- **Commit 4 - chat completions as JSON.** `wire.rs`, `tokens.rs`, and `reply.rs` with the chat rule and the echo model. It covers `max_tokens` and `max_completion_tokens`, `stop`, `seed`, and content given as a string or as parts. Errors: malformed JSON and missing `messages` get a 400, an unknown model gets a 404, and `n` above 1 gets a 400. Tests: a greeting gets a chat reply with `finish_reason: "stop"` and usage that adds up; the same request twice gives the same bytes, in one process and in two; `max_tokens: 2` cuts the reply to two tokens with `length`; the echo model returns the prompt lines; each error has OpenAI's envelope; `async-openai` parses the reply.
- **Commit 5 - streaming.** `sse.rs`: a role chunk, one content delta per token, a finish chunk, a usage chunk when `stream_options.include_usage` is set, then `[DONE]`. Tests: the exact frame order; the deltas joined equal the JSON reply; the delta count equals `completion_tokens`; the usage chunk appears only on request; `async-openai` reads the stream to its end.
- **Commit 6 - JSON from a schema.** `schema.rs` follows the `TestModel` recipe in section 3 of the report: `const`, `enum`, `allOf`, `$ref`, `anyOf` and `oneOf`, then by type, with required properties only, canned values per string `format`, and a depth limit of 12. Rule 4 of the ladder uses it. Tests: one per schema feature; every generated value validates against its schema, judged by the `jsonschema` crate; a `json_schema` request gets valid JSON as JSON and as a stream.
- **Commit 7 - tool calls.** `directive.rs` and rules 1 to 3 of the ladder. Tool calls go out in the JSON reply and in the stream, where the name goes first and the arguments follow in token-sized pieces. A tool message that answers no earlier call gets a 400. Tests: a directive call with exact arguments; a call with generated arguments that validate; two calls in one turn; a three-turn script that calls, calls, then says; tool results quoted and the loop ended; a named `tool_choice`; `required`; the three spellings of a tool's name; `none` leading to chat; nested JSON arrays in user text read as plain text; a broken directive and an unpaired tool message each getting a 400; a streamed call that reassembles to the same arguments; `async-openai` driving a full tool loop.
- **Commit 8 - one bot per growing conversation.** `brain.rs` keeps up to 64 finished bots, keyed by a hash of the seed and the user turns so far. A request whose turns extend a kept conversation takes that bot, runs one turn, and puts it back under the new key. A bot that panicked is dropped. Tests: the second request of a growing conversation runs one bot turn; replies match with the cache on and off; two identical requests at once get the same bytes.
- **Commit 9 - README and AGENTS.** The full `README.md`: what CheapSeek is, running it, pointing a client or a Gateway `[[model]]` entry at it, the two models, the reply rules with a worked tool loop, what is repeatable and its two limits (date and time answers follow the host clock and time zone, and a few sums follow the system's math library), what it leaves out, running the tests, and the notice for RustBot and PyBot (both Apache-2.0). `AGENTS.md` gets its final rules. Then **push 2** and watch CI to green.
- **Commit 10 - the record.** Add "What the build found" and the todo statuses to the repo copy of the plan, with both gates' numbers and both CI results. Then **push 3**.

## Verification and commit messages

- Each code commit passes, with exit 0 on every line: `cargo fmt --check`, `cargo clippy --all-targets --locked`, `cargo test --locked`, `cargo doc --no-deps --locked` under `RUSTDOCFLAGS=-D warnings`, and `cargo deny check`.
- Each message is written from the staged diff by `.cursor/rules/commit.mdc`, with `Plan: /plan/2026-11-22-1-cheapseek-server.plan.md` as its last line.
- Pushes go straight to `cppalliance/cheapseek`, so a failed check is fixed by a new commit and history stays as pushed.

## What each step needs

- The local gate needs only the `RustBot/` checkout. It delivers the build time, the reply times, and the length of the cut that commit 3 uses.
- Commit 0 needs this plan's text. Commit 1 needs the report on disk and the six research files.
- Commit 2 needs the house files it copies from: the Gateway's lint tables and workflow pins, and `vibelight`'s single-package layout.
- Commit 3 needs commit 2's package and runtime, and the cut length from the local gate. It delivers the reply function and the measured numbers.
- Commit 4 needs the reply function and the router. It delivers the reply plan, the token rule and the wire types.
- Commits 5 and 6 each need commit 4 only. Commit 7 needs both: the stream encoder from 5 and the generator from 6.
- Commit 8 needs commit 3 only, including its proof that a `Bot` is `Send`. It sits after commit 7 so the features land first.
- Commit 9 needs the final behavior. Commit 10 needs both gates' numbers and both CI results.
- Running steps side by side gains nothing here: there is one working tree, and each step ends in a full build.
- Commit 7 is the largest step. If it grows past a day's work, it splits into directives first and the `tool_choice` and name rules second.

## Choices I settled

- RustBot at a pinned commit, and pushes after commit 3 and at the end, both per your answers.
- The local RustBot checkout is built and timed before commit 0 (confidence high: it moves the biggest unknown ahead of all the work, for a few minutes of build).
- `cargo test` over nextest (confidence medium: tests in one process share the bot's warm tables).
- Dependencies optimized in the dev profile (confidence medium: it keeps the bot fast in tests, for one longer cold build).
- The printed line is the base URL (confidence medium: it pastes straight into a client's base URL, and the port is in it).
- A tool is called only when the request asks for one (confidence medium: a harness under test runs real tools).
- Only `[[call ` and `[[say ` open a directive (confidence medium: nested JSON arrays in user text stay plain text).
- Unknown request fields are ignored, and an unknown model is an error (confidence medium: the Harness sends `chat_template_kwargs`, and a misspelled model name should fail loudly).
- `n` above 1 is refused (confidence low: rare in agent traffic, cheap to add later).
- `async-openai` and `jsonschema` as test-only judges (confidence medium: independent checks of the wire format and of generated JSON).
- A bot panic becomes `Hello!` (confidence medium: it is PyBot's own fallback line).
- User text reaches the bot as one line (confidence medium: the bot's own CLI feeds it one line per turn, and it takes line-ending differences between systems out of the reply).
- Chat replies pass through `rustbot::one_line` (confidence medium: it is what the bot's CLI prints).

## Not in this plan

- Faults and pacing as directives, a call-every-tool model, embeddings and the Responses API. The report lists them under "Build next".
- A fork of RustBot. The local read shows it would be small: one clock helper and 14 one-line swaps pin the date, and a `cli` feature takes `clap` and `rustyline` out of the library build.
- Wiring the Gateway, Harness and Workshop tests to CheapSeek.
- Release binaries and an installer.

## Risks

- RustBot's build cost is a prediction from reading: a few minutes per system and profile on four cores, for one 29.8 MB crate that is mostly rule tables. The local gate measures it here, and commit 3 measures it in CI, before anything depends on it.
- The panic paths found by reading all need very long input: the backtrack limit inside `fancy-regex`, and a regex built per call from one giant word. The cut covers them, and the guard covers what reading missed.
- About 6,900 regexes compile at first use, so the first question on a new topic costs more than a warm one. Both gates record that cost.
- Date and time answers follow the host clock and time zone, and a few sums use the system's math library, so their last digit can differ between Windows, Linux and macOS. The tests ask about other things.
- `async-openai` may be stricter or looser than OpenAI itself in places. Where they disagree, OpenAI's API reference wins and the record says so.
- The lint set is strict, with pedantic at deny, so the first clippy run may ask for small changes.
