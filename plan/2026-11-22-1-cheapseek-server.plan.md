---
name: CheapSeek Test Server
overview: "Build CheapSeek in `cheapseek/` as one Rust executable: an Axum server with a strict OpenAI chat-completions API, RustBot for chat text, and rule-driven tool calls. A build-and-timing check of the local RustBot checkout comes first, then eleven commits on `master`, each passing the whole verification list, with CI on Windows, Linux and macOS."
todos:
  - id: local-gate
    content: "Before commit 0: build the local RustBot checkout in release mode into a scratch target folder, run its own tests, time its CLI at three message lengths, compare two runs of one conversation, apply the stop rules"
    status: completed
  - id: plan-copy
    content: "Commit 0: copy the approved plan alone to cheapseek/plan/2026-11-22-1-cheapseek-server.plan.md and commit it on master"
    status: completed
  - id: research-report
    content: "Commit 1: move the six research files into cheapseek/research, add design/state-of-practice.md, check the references, commit"
    status: completed
  - id: scaffold-ci
    content: "Commit 2: package, lint, deny and CI files, AGENTS.md, short README, a server that prints its URL and serves the model routes and the 404 envelope, tests, verify, commit"
    status: completed
  - id: rustbot-gate
    content: "Commit 3: pin RustBot, write brain.rs with the one-line cut, the panic guard, deep stacks and warm start, recorded-reply tests, measure build and reply times, verify, commit, push 1, watch CI on three systems, apply the gate"
    status: completed
  - id: chat-json
    content: "Commit 4: chat completions as JSON with the chat rule, the echo model, the token rule, limits, seed and errors, tests including async-openai, verify, commit"
    status: completed
  - id: streaming
    content: "Commit 5: SSE streaming with the role chunk, token deltas, finish chunk, usage chunk on request and [DONE], tests, verify, commit"
    status: completed
  - id: schema-json
    content: "Commit 6: schema.rs generator and response_format support, tests judged by the jsonschema crate, verify, commit"
    status: completed
  - id: tool-calls
    content: "Commit 7: directive.rs and ladder rules 1 to 3, tool calls in JSON and in the stream, the pairing check, tests including a full tool loop through async-openai, verify, commit"
    status: completed
  - id: bot-cache
    content: "Commit 8: keep up to 64 finished bots for growing conversations, tests, verify, commit"
    status: completed
  - id: readme
    content: "Commit 9: full README.md and final AGENTS.md, verify, commit, push 2, watch CI to green"
    status: completed
  - id: record
    content: "Commit 10: add What the build found and the todo statuses to the plan copy, commit, push 3"
    status: completed
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

# What the build found

The plan held. CheapSeek builds, every check passes at every commit, and CI is green on Ubuntu, Windows and macOS. Neither gate stopped the work, so RustBot stays. This record was written after push 2 was green and before push 3. The points below differ from the plan above, or add to it.

## The result

- **Tests:** 113 unit tests, 77 tests of the public behavior over HTTP, and 1 doc test, all passing.
- **CI on push 2, run 45072798348:** every job green. The test jobs took 4.1 minutes on Ubuntu, 7.6 on Windows and 5.3 on macOS. Clippy took 1.5 minutes.
- **Commits:** twelve where the plan counted eleven. The extra one corrects a time in the README after push 2.
- **Speed, release build, one Windows machine:** about 2 s from start to the printed URL, about 1.4 ms for a reply made by rule, and about 20 ms for a new conversation of two user turns.

## The local gate, before commit 0

- **RustBot built in release mode in 167.4 s,** and its executable is 44,989,440 bytes.
- **803 of its own tests passed, 1 is ignored, and 0 failed.** The whole run took 490.8 s, most of it the unoptimized tests. That is why the dev profile of CheapSeek optimizes dependencies.
- **The first reply of a process took 2.3 to 2.8 s here.** RustBot's README gives 375 ms on its own machine. The bot compiles most of its rules on its first turn.
- **Warm short replies took 4 to 35 ms, with a median of 26 ms,** over 30 turns in growing conversations with a new bot for each conversation. The first use of another topic took 43 to 567 ms.
- **A 500 character line took 142 to 158 ms, and a 2,000 character line 475 to 494 ms.** That is over the plan's 150 ms, so the cut is 500 characters.
- **A short question right after a 2,000 character line took 554 ms,** because the bot reads earlier lines of the conversation again. The cut bounds that cost too.
- **Two processes gave the same replies and the same full traces** for a four-line conversation at seed 0.
- **Every stop rule stayed clear.**

## Commit 0, `2eb0fae`: the plan

- **The copy is two statuses ahead of the approved plan.** Cursor's todo list had written `completed` for the local gate and `in_progress` for the plan copy into the plan file before the copy was made.

## Commit 1, `aaf1ca3`: research and report

- **The six research files moved in from the workspace's staging folder,** and the report went in as it was written. Its two references to the `research` folder resolve.

## Commit 2, `d79ccb6`: scaffold and CI

- **A wrong method on a known path gets a 405 in the error envelope.** The plan named the 404 for unknown paths alone.
- **`serve` turns on `TCP_NODELAY`,** so the small frames of a stream leave at once.
- **`clippy.toml` bans the hash collections and the clock reads,** as `vibelight` does, because either could change a reply between runs.
- **`rust-version` is 1.99,** the pinned toolchain, since that is the one version CI builds with.
- **The test of `--port` holds a port and expects the executable to fail on it.** A test that hands over a free port can lose it to another test.
- **The owner pushed commits 0 to 2 before push 1.** CI run 45067315304 on commit 2 was green in 1.5 minutes.

## Commit 3, `5fb2ef7`: RustBot and the CI gate

- **Cold builds here: 161.6 s in the dev profile and 138.8 s in release.** The executables are 46.9 MB and 45.6 MB.
- **Start to the printed URL: 2.0 s in release and 2.5 s in dev.** The process held 292 MiB after the warm-up turn and 386 MiB after a few dozen requests. RustBot's README gives 166.6 MiB.
- **Warm one-turn replies in the test profile took 2.6 to 16.9 ms, with a median of 9.5 ms,** over 40 turns. The first use of a topic took 10 to 579 ms, and a turn cut to 500 characters took 167.5 ms.
- **`Cargo.lock` keeps four crates at the versions of RustBot's own lock file:** `regex` 1.13.1, `regex-automata` 0.4.18, `regex-syntax` 0.8.11 and `smallvec` 1.16.5. The plan pinned the commit alone. This way the bot runs on the releases its 803 tests ran on.
- **`cargo build` and `cargo test` each build RustBot.** The dev-dependencies turn on more features of `syn`, which `serde_derive` uses, so the two commands resolve the bot's dependencies differently. CI is unaffected, because each job runs one of them.
- **CI run 45067510507 was green on all three systems.** The cold test jobs took 4.2 minutes on Ubuntu, 4.9 on Windows and 6.4 on macOS, and the recorded replies matched on each.
- **The gate passed on all four counts:** RustBot builds on the three systems, the recorded replies agree between runs and systems, the slowest warm reply was 16.9 ms against a limit of 200 ms, and the longest cold test job was 6.4 minutes against a limit of 45.

## Commit 4, `8f72db1`: chat completions as JSON

- **`serde_path_to_error` names the field at fault,** so the message of a 400 reads like `Invalid 'messages[1].role': unknown variant ...`. It was in the tree already, through axum.
- **The body limit is 64 MiB.** The default of the HTTP library is 2 MiB, and a test of a long conversation sends more.
- **More than four stop sequences, and a token limit of 0, each get a 400,** as OpenAI answers them.
- **The seed changed none of the replies the tests ask for.** The bot draws on it in three rules alone. The tests therefore check that a seed is accepted, and that it changes the id.
- **A request for a stream was refused for this one commit.** Commit 5 took the refusal out.
- **`async-openai` reads `created` as a 32-bit number.** The constant 1795305600 fits.

## Commit 5, `0df2a23`: streaming

- **`futures-util` is a direct dependency now.** It turns the list of events into a response body, and it was in the tree already.
- **`stream_options` without `stream` gets a 400,** as OpenAI answers it.

## Commit 6, `c54a29d`: JSON from a schema

- **The recipe reads `examples` too,** after `enum`, as `TestModel` does.
- **Three more bounds stand beside the depth limit of 12:** 1,000 values for each reply, 64 items for each array, and 256 letters for each string. A schema that asks for a million items would otherwise fill the reply.
- **The seed picks the letter of a string,** so the items of a list that must differ can differ.
- **A test at the largest seed found two equal items in such a list.** The count of the items wrapped around. The recipe now reduces the seed to 32 bits first.
- **A `json_schema` format without a `name` gets a 400,** as OpenAI requires the name.

## Commit 7, `7d4e1fb`: tool calls

- **It stayed one commit.**
- **The echo model shows the tools, the tool choice, the calls and the results,** and the prompt tokens count them. The plan gave the echo one line for each message.
- **The pairing check runs both ways.** A tool message without an earlier call gets a 400, as planned. A call without a later tool message gets one too, because OpenAI refuses both.
- **`tool_choice` without `tools` gets a 400,** as OpenAI answers it.
- **The token limit cuts the arguments of a call,** with the finish reason `length`. A harness must refuse to run such a call, and now a test can send it one.
- **A directive calls its tool even under `tool_choice: "none"`,** and a tool the request does not offer gets empty arguments. The test wrote the directive, so the directive decides.
- **The first chunk of a stream holds `null` content before tool calls** and empty content before text, as OpenAI sends them.

## Commit 8, `2f6d74d`: one bot for each growing conversation

- **A kept bot is found by the seed and the exact turns it read.** The plan keyed it by a hash of them. Exact text costs a little memory, and no collision can hand a request the wrong bot.
- **A kept bot is boxed,** so it moves in and out of the list cheaply.
- **Each router has a brain of its own,** so two servers in one process share no bot.
- **The lock is free while a bot runs.** A slow turn of one conversation never holds up another.

## Commit 9, `4c400c1`, and the correction, `5e4d8dc`

- **The sample replies in the README are the output of the release build.**
- **RustBot answers a request for code with a general line** (`Go on - what's been happening?`). PyBot said that it did not know enough.
- **The Gateway entry in the README follows the Gateway's own example config and its config tests.** It is untested, because this plan left the Gateway out.
- **The README first said four to seven minutes for a cold CI test job.** The Windows job of push 2 took 7.6, so `5e4d8dc` changed the text to four to eight.

## The risks, looked at again

- **RustBot's build cost** is about two and a half minutes cold on this machine, and four to eight minutes for a cold CI test job. The prediction from reading was a few minutes.
- **Every test and every sample ran without a panic of the real bot.** The guard is tested with a closure that panics.
- **The first question on a new topic** cost up to 0.6 s, as predicted.
- **Hash order inside RustBot** reached no reply: the recorded replies are the same on three systems and in every run.
- **`async-openai` and OpenAI's own shapes** agreed everywhere the tests look. The typed client read every kind of reply: JSON, stream, tool calls, a streamed tool call and the error envelope.
- **The lint set** asked for small changes in most commits, such as a boxed bot, an enum in place of a nested `Option`, and `#[expect]` on the `usage` field names.

## Still open when this record was written

- **Push 3 and its CI run.** They come after this commit, so the chat reports them.
- **A panic of the real bot** is still untested, because no input that makes one is known.
- **A Gateway that runs against CheapSeek,** and the Harness and Workshop tests that would use it. The plan left that wiring out.
- **The speed numbers come from one machine.** CI gives build times for three systems, and reply times are measured here alone.
