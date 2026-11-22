# CheapSeek

CheapSeek is a test server that speaks the OpenAI chat-completions API and answers by rule, with zero inference. It is fast and local, and it gives the same reply to the same request every time, so the tests of a gateway or an agent harness can use it in place of a real model.

This is early work. Today the server answers chat completions as one JSON object, with chat text from a rule-based bot. Streaming and tool calls come next.

## Run

```sh
cargo run --release
```

The server binds a free port on `127.0.0.1`, prints one line such as `http://127.0.0.1:54321/v1`, and serves until Ctrl-C. That line is the base URL an OpenAI client takes. `--port N` picks the port.

```sh
curl http://127.0.0.1:54321/v1/models
```

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo test --locked
cargo doc --no-deps --locked
cargo deny check
```

## Design

[design/state-of-practice.md](design/state-of-practice.md) reports what other fake LLMs do and what CheapSeek adopts. The `research` folder holds the evidence behind it.

## License

Boost Software License 1.0. See [LICENSE](LICENSE).
