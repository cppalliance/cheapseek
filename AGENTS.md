# CheapSeek

CheapSeek is a test server that speaks the OpenAI chat-completions API and answers by rule, with zero inference. Tests of gateways and agent harnesses point at it in place of a real model. This repository holds one Rust package: a library with the server, and a thin executable around it.

## Standing rules

- **Strict OpenAI.** The API is the OpenAI API and nothing more: the same routes, fields, status codes and error envelope. A test steers the server through the model name and the prompt text. Custom headers, admin routes and config files stay out.
- **Same request, same bytes.** A reply is a pure function of the request body. Everything a rule reads is in the request. Counters, queues, clocks and random sources stay out of the server, and a cache may save time but must never change a reply.
- **One process, many tests.** Any number of tests may share one server at the same time, in any order.
- **Open on loopback.** The executable binds `127.0.0.1` and answers any caller there. An `Authorization` header is accepted and ignored.
- **Comments** explain a non-obvious reason: a constraint, an ordering requirement or a workaround.

## Rust

- Return `Result` for every expected failure. A panic means a bug.
- Lint levels live in the `[lints]` tables of `Cargo.toml`, and `.cargo/config.toml` turns every warning into an error.
- Document every public item, with `# Errors` and `# Panics` where they apply.
- Production code uses the standard module layout: `foo.rs`, and never `mod.rs`.
- Maps and sets are B-tree based, and the code reads no clock. `clippy.toml` bans the hash collections and the clock reads, because a hash seed or a clock value in a reply would break the rule above.
- Add a test in the same change as the code. Unit tests go in `foo-tests.rs` beside the file under test, wired with `#[path = "foo-tests.rs"] mod tests;`. Tests of the public behavior go in the one binary at `tests/it/main.rs`, one topic per file, and each talks to a real server over HTTP.
- A test name is a sentence that states the behavior.
- Confirm that a crate is the intended, existing package before adding it to `Cargo.toml`.

## Checks

- From the repository root: `cargo fmt --check`, `cargo clippy --all-targets --locked`, `cargo test --locked`, `cargo doc --no-deps --locked` with `RUSTDOCFLAGS` set to `-D warnings`, and `cargo deny check`.
- CI runs all of these on every pull request and on `master`, in `.github/workflows/ci.yml`. The tests run on Ubuntu, Windows and macOS. A job added there also goes into the `needs` list of `ci-green`, the one check to require.

## Plans

- **A plan runs from its copy in `plan/`.** Before the first step, copy the approved plan verbatim to `plan/YYYY-MM-DD-N-words.plan.md`: the date the run starts, `N` the next unused number for that date starting at 1, and one to four kebab-case words from the title. The copy is the first commit of the plan.
- **Each commit of a plan names the copy.** The last line of the message is `Plan: /plan/` followed by the file name of the copy.
- **The last commit of a plan records what the build found.** The copy gains a section with the result and with each point where the build differs from the plan.

## Design and research

- `design/state-of-practice.md` is the report the design follows: what other fake LLMs do, and what CheapSeek adopts.
- `research/` holds the evidence behind the report, one file per family of practice.
