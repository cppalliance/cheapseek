# CheapSeek

CheapSeek is a test server that speaks the OpenAI chat-completions API and answers by rule, with zero inference. Tests of gateways and agent harnesses point at it in place of a real model. This repository holds one Rust package: a library with the server, and a thin executable around it. `README.md` is the user's guide, and it describes every behavior a test can rely on.

## Standing rules

- **Strict OpenAI.** The API is the OpenAI API and nothing more: the same routes, fields, status codes and error envelope. A test steers the server through the model name and the prompt text. A new way to steer it is a new model name or a new directive. Custom headers, admin routes and config files stay out.
- **Same request, same bytes.** A reply is a pure function of the request body. Everything a rule reads is in the request. Counters, queues, clocks and random sources stay out of the server, and the kept bots in `src/brain.rs` may save time but must never change a reply.
- **One process, many tests.** Any number of tests may share one server at the same time, in any order.
- **Open on loopback.** The executable binds `127.0.0.1` and answers any caller there. An `Authorization` header is accepted and ignored.
- **Every loop ends.** A request that ends with tool results gets text. A rule that could call a tool again and again needs a way out that the request controls.
- **The README moves with the code.** A change to what a request gets also changes `README.md`, in the same commit.
- **Comments** explain a non-obvious reason: a constraint, an ordering requirement or a workaround.

## Where each decision lives

- `src/reply.rs` holds the reply rules and their order, the echo rendering, the cut at the token limit, and the ids.
- `src/directive.rs` reads the directives a test writes into the prompt.
- `src/schema.rs` makes a JSON value from a JSON Schema, for tool arguments and for structured output.
- `src/tokens.rs` is the token rule. Usage, the token limit and the stream all count with it.
- `src/sse.rs` writes a reply as a stream of server-sent events.
- `src/wire.rs` holds the JSON shapes of requests and replies, and the checks of a request body.
- `src/brain.rs` wraps RustBot: the one-line cut of a turn, the replay, the kept bots, and the guard against a panic.
- `src/api.rs` holds the routes and turns each failure into the error envelope.

## The bot

- **RustBot is pinned.** `Cargo.toml` names one commit of it, and `Cargo.lock` keeps `regex`, `regex-automata`, `regex-syntax` and `smallvec` at the versions that RustBot's own lock file names. After a `cargo update`, pin those four again with `cargo update -p <name> --precise <version>`.
- **The recorded replies are the specification.** `src/brain-tests.rs` and `tests/it/conversations.rs` hold replies the bot gave, word for word, and CI compares them on three systems. Change one only for an intended change, such as a new pin, in the same commit, after reading its diff. Never record again to make a failing test pass.
- **Test conversations stay clear of dates and sums.** Those two kinds of answer follow the host clock and the system's math library.
- **Every profile unwinds on panic.** The server catches a panic inside the bot and answers with the bot's fallback line. `panic = "abort"` would end the process there.
- **The bot is built twice on a developer machine.** `cargo build` and `cargo test` resolve different features for its dependencies, so each builds it once, at about two and a half minutes. The dev profile optimizes dependencies, because the unoptimized bot takes seconds per reply.

## Rust

- Return `Result` for every expected failure. A panic means a bug.
- Lint levels live in the `[lints]` tables of `Cargo.toml`, and `.cargo/config.toml` turns every warning into an error.
- Document every public item, with `# Errors` and `# Panics` where they apply.
- Production code uses the standard module layout: `foo.rs`, and never `mod.rs`.
- Maps and sets are B-tree based, and the code reads no clock. `clippy.toml` bans the hash collections and the clock reads, because a hash seed or a clock value in a reply would break the rule above.
- JSON objects are written with sorted keys. `deny.toml` bans the `preserve_order` feature of `serde_json`, which a dependency could otherwise turn on for the whole build.
- Confirm that a crate is the intended, existing package before adding it to `Cargo.toml`.

## Tests

- Add a test in the same change as the code. Unit tests go in `foo-tests.rs` beside the file under test, wired with `#[path = "foo-tests.rs"] mod tests;`. Tests of the public behavior go in the one binary at `tests/it/main.rs`, one topic per file.
- A test of the public behavior starts a server of its own on a free loopback port and talks to it over HTTP. `tests/it/process.rs` starts the built executable.
- A test name is a sentence that states the behavior.
- Two crates that this project does not write are the judges. `async-openai` reads every kind of reply the server makes, and `jsonschema` validates every value made from a schema. A new kind of reply gets a test through `async-openai`.
- A test waits on the server's answer and never sleeps.

## Checks

- From the repository root: `cargo fmt --check`, `cargo clippy --all-targets --locked`, `cargo test --locked`, `cargo doc --no-deps --locked` with `RUSTDOCFLAGS` set to `-D warnings`, and `cargo deny check`.
- CI runs all of these on every pull request and on `master`, in `.github/workflows/ci.yml`. The tests run on Ubuntu, Windows and macOS. A job added there also goes into the `needs` list of `ci-green`, the one check to require.
- Never push a tag. Push `master` when the owner has said to push.

## Plans

- **A plan runs from its copy in `plan/`.** Before the first step, copy the approved plan verbatim to `plan/YYYY-MM-DD-N-words.plan.md`: the date the run starts, `N` the next unused number for that date starting at 1, and one to four kebab-case words from the title. The copy is the first commit of the plan.
- **Each commit of a plan names the copy.** The last line of the message is `Plan: /plan/` followed by the file name of the copy.
- **The last commit of a plan records what the build found.** The copy gains a section with the result and with each point where the build differs from the plan.

## Design and research

- `design/state-of-practice.md` is the report the design follows: what other fake LLMs do, and what CheapSeek adopts. Its "Build next" list names the features that come after the first plan.
- `research/` holds the evidence behind the report, one file per family of practice.
