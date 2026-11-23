# CheapSeek

CheapSeek is a test server that speaks the OpenAI chat-completions API and answers by rule, with zero inference. Tests of gateways and agent harnesses point at it in place of a real model. This repository holds one Rust package, a library with the server and a thin executable around it, and a GitHub Action that starts the executable in a job. `README.md` is the user's guide, and it describes every behavior a test can rely on.

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
- `src/brain.rs` wraps RustBot: the one-line cut of a turn, the fixed time of the bots, the replay, the kept bots, and the guard against a panic.
- `src/api.rs` holds the routes and turns each failure into the error envelope.
- `action.yml` and `action/start.sh` are the GitHub Action. The script picks the executable for the runner, downloads it from the release, compares its SHA-256 sum, starts it, and gives the base URL to the job.
- `action/check.sh` compares the replies of a running server with the recorded replies in `action/check/`.
- `.github/workflows/release.yml` builds, checks and publishes the executables of a release, and moves the major tag.

## The bot

- **RustBot is pinned, and it is built from the fork `cppalliance/rustbot`.** `Cargo.toml` names one commit of the fork, and `Cargo.lock` keeps `regex`, `regex-automata`, `regex-syntax` and `smallvec` at the versions that RustBot's own lock file names. After a `cargo update`, pin those four again with `cargo update -p <name> --precise <version>`.
- **The recorded replies are the specification.** `src/brain-tests.rs` and `tests/it/conversations.rs` hold replies the bot gave, word for word, and CI compares them on three systems. Change one only for an intended change, such as a new pin, in the same commit, after reading its diff. Never record again to make a failing test pass.
- **The bot's clock is fixed.** `src/brain.rs` gives every new bot the `created` time, and the fork reads time zones from inside the executable. A new pin of the fork must keep both changes: `tests/it/dates.rs` and `action/check/clock.request.json` hold replies that fit that one moment alone.
- **Test conversations stay clear of sums.** A few of the bot's sums follow the system's math library. A question about a date is safe.
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
- `action/check/` holds three requests and the whole reply to each, byte for byte. CI sends them to the debug build on three systems, and the release workflow sends them to each release build. A change to the bytes of a reply changes the recorded file in the same commit, after reading its diff.

## Checks

- From the repository root: `cargo fmt --check`, `cargo clippy --all-targets --locked`, `cargo test --locked`, `cargo doc --no-deps --locked` with `RUSTDOCFLAGS` set to `-D warnings`, and `cargo deny check`.
- CI runs all of these on every pull request and on `master`, in `.github/workflows/ci.yml`. The tests run on Ubuntu, Windows and macOS. A job added there also goes into the `needs` list of `ci-green`, the one check to require.
- CI also runs `shellcheck` on the scripts in `action/`, and on each of the three systems it starts a server through the action and runs `action/check.sh`.
- Push `master` when the owner has said to push. The next section holds the rules for tags.

## Releases

- **The version lives in `Cargo.toml` alone.** `action/start.sh` reads it there to find the release it downloads, so the action and the executable always come from one commit.
- **A release is the tag `vX.Y.Z` on a commit of `master` with green CI,** pushed when the owner has said to release. The tag names the version in `Cargo.toml`, and `release.yml` fails a tag that names another. The commit that sets the version also updates `Cargo.lock`.
- **A version tag is never moved and never used again.** A release that went wrong gets a fix on `master` and the next version.
- **`release.yml` alone moves the major tag `v1`,** after the published files have worked through the action on three systems. Never push `v1` by hand.
- **A manual run of the Release workflow builds and checks without publishing.** Run it before a tag when the workflow or the action changed.
- **A new major version** changes the reference `cppalliance/cheapseek@v1` in the `proof` job of `release.yml` and in `README.md`.
- **A new system** is one row in the `build` matrix of `release.yml`, one name in its `publish` job, one line in `action/start.sh`, and one sentence in `README.md`.

## Plans

- **A plan runs from its copy in `plan/`.** Before the first step, copy the approved plan to `plan/YYYY-MM-DD-N-words.plan.md`: the date the run starts, `N` the next unused number for that date starting at 1, and one to four kebab-case words from the title. The copy is the first commit of the plan.
- **The copy is verbatim but for the paths of the owner's machine,** which lead nowhere for a reader on GitHub. A link to a file of this repository becomes a relative link. A link to any other file on the machine becomes the text of the link. A bare path becomes the name of its file or folder.
- **Each commit of a plan names the copy.** The last line of the message is `Plan: /plan/` followed by the file name of the copy.
- **The last commit of a plan records what the build found.** The copy gains a section with the result and with each point where the build differs from the plan.

## Design and research

- `design/state-of-practice.md` is the report the design follows: what other fake LLMs do, and what CheapSeek adopts. Its "Build next" list names the features that come after the first plan.
- `research/` holds the evidence behind the report, one file per family of practice.
