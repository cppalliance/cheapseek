---
produced: 2026-11-22
title: "Record and replay and caching of LLM traffic for tests - HTTP cassettes (VCR.py, go-vcr, rvcr, reqwest-vcr, httpmock, cassetter), LLM-specific recorders (retake, llm-vcr, cagent, Goose TestProvider), cache layers, agent trajectory replay, request matching keys, SSE streaming, tool-call ids, secrets scrubbing, cassette drift, Rust options"
---

# Record and replay, and caching, of LLM traffic for tests

This is a survey of how people record a real model's replies once and replay them in tests, which tools exist, how they match a request to a recording, and what goes wrong. The target reader is someone building a fake LLM for a Rust agent harness and gateway that speak the OpenAI chat-completions API (plain JSON and SSE, with tool calls).

## How to read this file

- Every fact below comes from a page I fetched, or a search excerpt I read, on 2026-11-22. Each section ends with a "How confirmed" bullet.
- "Fetched" means I opened that exact URL and read the page text.
- "Search excerpt" means the search tool returned a long excerpt of the page, but I did not open the URL myself. Treat those as one notch weaker.
- "GitHub API" means the license, language, and last push date came from the GitHub repository metadata.
- Command line flags are written without their leading dashes (for example `record-mode=none`), to keep this file free of double dashes. Add the two dashes back when you type them.
- Things I looked for and could not confirm are collected in the section "What I could not confirm", just before the closing lessons.

## Part 1 - General HTTP cassette tools

These record raw HTTP request and response pairs to a file (a "cassette") and replay them later. They treat LLM traffic as plain HTTP. They are the base that most LLM projects start from.

### VCR.py

- URL: https://github.com/kevin1024/vcrpy and https://vcrpy.readthedocs.io/en/latest/
- Language: Python
- License: MIT (GitHub API)
- Version seen: docs are for vcrpy 9.4.2
- How matching works: the `match_on` setting. The default is `['method', 'scheme', 'host', 'port', 'path', 'query']`, which means same URL and same method. The request body is left out of the default. Other built-in matchers are `uri`, `raw_body`, `body` (body parsed by content type, so JSON key order stops mattering), and `headers`. You can register your own matcher function that gets the two requests and returns a bool or raises an assertion.
- Why that default matters for LLMs: every chat-completions call is a POST to the same URL, so with the default matcher any recording matches any prompt. See the Pydantic AI and evalship sections for what that costs.
- Sequence behaviour: by default each recorded response can be played back only once per cassette use, so three identical requests replay the three recorded responses in order. `allow_playback_repeats` lets one recording answer many times.
- Record modes: `once` (default: replay, record only when the cassette file is missing, raise an error on a new request when the file exists), `new_episodes` (replay what matches, record the rest), `none` (replay only, raise on anything new), `all` (always record, never replay).
- What happens on a miss: in `once` with an existing cassette and in `none`, the request raises an error. In `new_episodes` and `all` it goes to the real server and is recorded.
- Streaming: VCR.py stores a whole response body. An open issue (number 989, opened 2026-05-12, last comment 2026-10-27) reports that the httpx transport adapter joins the entire response stream before returning it, so a client waiting for the first SSE event times out while a cassette is active. A maintainer comment on the older issue 597 says async httpx streaming with `client.stream(...)` plus `aiter_bytes` or `aiter_lines` records and replays correctly from v8.2.0. Put together: a finite SSE reply (like a chat completion) replays as one buffered body, and chunk boundaries and timing are lost.
- Tool-call rounds: nothing special. Each round is one more POST. With the default matcher they replay in recorded order because each recording plays once.
- Secrets: `filter_headers`, `filter_query_parameters`, `filter_post_data_parameters` (top-level JSON keys, or nested keys with `filter_nested_post_data_parameters=True`), `filter_sensitive_data` (swap a secret for a placeholder everywhere, and swap it back on load so matching still works), `before_record_request`, `before_record_response`, plus save and load hooks.
- Compressed bodies: response bodies are stored compressed unless you set `decode_compressed_response=True`. The docs warn that a secret inside a compressed body is not found by the placeholder filter.
- Other useful bits: `record_on_exception=False` keeps a failed test from writing a cassette. `drop_unused_requests=True` prunes recordings the test no longer makes. `cassette.get_metadata(key, default)` stores a value such as a random seed in the cassette so record and replay runs use the same one. A load hook can rewrite old cassettes (for example a changed URL) without re-recording.
- YAML safety: cassettes load with a safe YAML loader that refuses Python object tags.
- Reported lessons: the secret filter is plain text replacement, so a value that is URL-encoded or JSON-escaped needs its own entry. In CI, where the real key is absent, give the same fallback key to your code and to the filter so requests still match.
- How confirmed: fetched https://vcrpy.readthedocs.io/en/latest/usage.html, https://vcrpy.readthedocs.io/en/latest/advanced.html, https://vcrpy.readthedocs.io/en/latest/configuration.html, https://github.com/kevin1024/vcrpy/issues/989. Issue 597 is a search excerpt (https://github.com/kevin1024/vcrpy/issues/597).

### pytest-recording

- URL: https://github.com/kiwicom/pytest-recording and https://pypi.org/project/pytest-recording/
- Language: Python (pytest plugin on top of VCR.py)
- License: MIT
- Version seen: 0.15.0, uploaded 2026-11-15
- How matching works: same as VCR.py. Config comes from a `vcr_config` fixture and from `@pytest.mark.vcr(...)` marks, merged from broad scope to narrow. A `pytest_recording_configure` hook gives you the VCR object for custom matchers.
- Default mode: `none`. The README says this is on purpose, to prevent unintentional network requests. You pass `record-mode=once` (or `rewrite`) on the command line to record.
- `rewrite` mode: rewrites the default cassette from scratch instead of appending.
- Network blocking: `@pytest.mark.block_network` or the `block-network` flag raises `RuntimeError: Network is disabled for host ...` on any socket or pycurl access. Allowed hosts are configurable.
- Cassette naming: `cassettes/{module_name}/{test_name}.yaml` next to the test module.
- What happens on a miss: with the default `none` mode, the test fails.
- The README says the older `pytest-vcr` plugin must be uninstalled first because the two are incompatible. The VCR.py docs say `pytest-vcr` is no longer actively maintained.
- How confirmed: fetched https://pypi.org/project/pytest-recording/ (full README).

### Ruby VCR

- URL: https://github.com/vcr/vcr and https://benoittgt.github.io/vcr/
- Language: Ruby
- License: GitHub API reports no detected SPDX id for this repo; I did not check the license file.
- How matching works: configurable on HTTP method, URI, host, path, body, and headers, or a custom matcher. An evalship article (below) states the default is method plus URI.
- Other features listed in the docs: YAML or JSON cassettes, ERB for dynamic responses, optional re-recording of cassettes on a configurable interval "to keep them fresh", and blocking all HTTP requests that you did not explicitly allow.
- The docs list ports to other languages, including one Rust entry named `http_replayer` (not checked further).
- How confirmed: fetched https://benoittgt.github.io/vcr/. GitHub API for repo metadata.

### Polly.js

- URL: https://netflix.github.io/pollyjs/ and https://github.com/Netflix/pollyjs
- Language: JavaScript
- License: Apache-2.0 (GitHub API). Last push 2025-05-31.
- How matching works: `matchRequestsBy` builds a GUID for each request from method, headers, body, URL parts, and `order`. All are on by default. Each can be turned off or replaced by a function that normalises the value (for example delete a header, or strip a field from the JSON body).
- Sequence behaviour: `order: true` by default, so the position of a request among identical requests is part of its identity.
- What happens on a miss: `recordIfMissing` defaults to `true`, so a miss goes to the server and is recorded. Set it to `false` to fail instead.
- Staleness: `expiresIn` (for example `'30d'`) plus `expiryStrategy` of `warn`, `error`, or `record`.
- Timing: `timing` controls replay delay. The default is `Timing.fixed(0)`. `Timing.relative(3.0)` replays at a multiple of the recorded time.
- Other: `recordFailedRequests` defaults to false, so persisting a response with status 400 or above throws. `keepUnusedRequests` defaults to false, so recordings that the test stopped making are removed.
- How confirmed: fetched https://netflix.github.io/pollyjs/#/configuration. GitHub API.

### nock (recorder and Nock Back)

- URL: https://github.com/nock/nock
- Language: JavaScript (Node)
- License: MIT (GitHub API)
- How it works: `nock.recorder.rec()` captures real calls. Nock Back wraps a test in a named JSON fixture.
- Nock Back modes: `wild` (everything goes to the internet), `dryrun` (default: use recorded nocks, still allow real HTTP, record nothing), `record` (use recorded, record new), `update` (delete and re-record), `lockdown` (use recorded, block all other HTTP).
- Request headers: left out of recordings by default. The README says recording them is "more trouble than it's worth" because some depend on timestamps or other values that change. User-agent is never recorded.
- Volatile bodies: the README shows a `filteringRequestBody` hook that rewrites a timestamp in the live body to the recorded one so the match holds. `afterRecord` post-processes what gets saved.
- How confirmed: fetched https://raw.githubusercontent.com/nock/nock/main/README.md.

### go-vcr

- URL: https://github.com/dnaeon/go-vcr
- Language: Go
- License: BSD-2-Clause
- Version seen: v4 (`gopkg.in/dnaeon/go-vcr.v4`). The README says moving from an older major version may require re-creating cassettes.
- How matching works: a `recorder.MatcherFunc(r *http.Request, i cassette.Request) bool`. The README's custom example compares method, URL, and body. A default matcher exists with options such as `WithIgnoreAuthorization()`.
- Hooks: `AfterCaptureHook`, `BeforeSaveHook`, `BeforeResponseReplayHook`, `OnRecorderStop`. The README warns that redacting in `AfterCaptureHook` changes what the running test sees (an OAuth token redacted there breaks the next request), so redact in `BeforeSaveHook` instead.
- Other: passthrough rules per request, `VCR_DEBUG=true` to trace why a request did not match, and an HTTP middleware for recording incoming requests on the server side.
- Used by: Docker cagent and Envoy AI Gateway (both below).
- How confirmed: fetched https://raw.githubusercontent.com/dnaeon/go-vcr/HEAD/README.md.

### WireMock (Java) record and playback

- URL: https://wiremock.org/docs/record-playback/
- Language: Java (standalone server with a JSON admin API)
- License: Apache-2.0 (GitHub API)
- How it works: WireMock proxies to the real API and turns the traffic into stub mappings, either by "recording" (start, stop) or "snapshotting" (convert requests already received).
- How matching works: each recorded request becomes a stub mapping. Request headers are part of the match only when you ask for them with `captureHeaders` (or `captureAllHeaders` with an exclude list). The body matcher is chosen from the content type: JSON bodies get `equalToJson` with `ignoreArrayOrder` and `ignoreExtraElements` both true, XML gets `equalToXml`, everything else gets exact `equalTo`. All of this is configurable.
- Sequence behaviour: `repeatsAsScenarios` (default true) puts identical requests with different responses into a Scenario, so playback returns the same series in order. After the series ends, the last response keeps being returned.
- Large bodies: `extractBodyCriteria` moves bodies over a size threshold into separate files.
- How confirmed: fetched https://wiremock.org/docs/record-playback/. GitHub API.

### Hoverfly

- URL: https://docs.hoverfly.io/
- Language: Go (a proxy)
- License: Apache-2.0 (GitHub API)
- Modes: capture, simulate, spy, synthesize, modify, diff. One mode at a time.
- Capture mode: Hoverfly runs as a proxy and records requests and responses. By default it ignores a duplicate request when the request has not changed. The `stateful` argument turns that check off so a sequence of different responses to the same request is captured and played back in order.
- How confirmed: fetched https://docs.hoverfly.io/en/latest/pages/keyconcepts/modes/modes.html and https://docs.hoverfly.io/en/latest/pages/keyconcepts/modes/capture.html. Docs version v1.12.18.

### mitmproxy server-side replay

- URL: https://docs.mitmproxy.org/stable/concepts/options/
- Language: Python (a proxy)
- License: MIT (GitHub API)
- How it works: `server_replay` loads saved flows and answers matching requests from them.
- How matching works: by default the request content, host, and port all count, and headers are ignored. Options loosen or tighten that: `server_replay_ignore_content`, `server_replay_ignore_host`, `server_replay_ignore_port`, `server_replay_ignore_params`, `server_replay_ignore_payload_params` (form fields only), and `server_replay_use_headers` (a list of headers that must match, empty by default).
- Sequence behaviour: a flow is removed from the replay state after use unless `server_replay_reuse` is set.
- What happens on a miss: `server_replay_extra` chooses `forward` (default: send to the real server), `kill`, or a fixed status of 204, 400, 404, or 500.
- `server_replay_refresh` (default true) adjusts date, expires, and last-modified headers and cookie expiry on replay.
- How confirmed: fetched https://docs.mitmproxy.org/stable/concepts/options/ and https://docs.mitmproxy.org/stable/mitmproxytutorial-replayrequests/.

### Cassetter

- URL: https://github.com/Kludex/cassetter, https://kludex.github.io/cassetter/, https://pypi.org/project/cassetter/, https://crates.io/crates/cassetter
- Language: a Rust core with SDKs for Python, Node/TypeScript, Go, and Rust. All SDKs share one cassette format.
- License: MIT
- Version seen: 0.17.0 on PyPI and on crates.io
- Why it is in this list: the Pydantic AI test guide now imports `from cassetter import Cassette`, so the largest LLM cassette suite I found runs on it.
- How matching works: default is method plus URI. Matchers are `method`, `uri`, `headers` (recorded headers must be a subset), `body`, and `json_body`. `ignore_json_paths` drops volatile JSON fields from the comparison. `uri_normalizer` rewrites both the recorded and the live URI before comparing.
- Sequence behaviour: each interaction is replayed at most once, in order. The docs add that a session that can record never replays an interaction it just recorded, so "the next turn of a conversation posting to the same URL" reaches the server.
- Record modes: `none`, `once`, `new_episodes`, `all`, `rewrite`, and `verify` (send matched requests live and fail if the live response differs from the recording).
- Secrets: filtered at write time. Headers such as `authorization`, `cookie`, `x-api-key`, and `api-key` are stripped automatically. The live request goes through the same filters before matching, so filtering never breaks replay.
- Concurrency: each `use_cassette` context is isolated with a context variable, so parallel async tasks can each have their own cassette.
- The tutorial's own advice: start with method plus URI, and add `json_body` only when one test sends several different payloads to the same URI.
- How confirmed: fetched https://kludex.github.io/cassetter/tutorial/matching/ and https://crates.io/crates/cassetter. PyPI page and GitHub README are search excerpts. See Part 2 for the Rust crate.

## Part 2 - Rust options

This is the part that maps most directly onto a Rust harness and gateway. There are three kinds of thing here: client-side middleware that wraps `reqwest`, mock servers that the system under test points its base URL at, and what two large Rust agents (Goose and Codex) built for themselves.

### rvcr

- URL: https://github.com/ChorusOne/rvcr and https://crates.io/crates/rvcr
- Language: Rust
- License: Apache-2.0
- Version seen: 0.2.1, last published 2024-03-05. It depends on `reqwest ^0.11` and `reqwest-middleware ^0.2.4`, so it is behind current reqwest.
- How it works: a `reqwest-middleware` middleware. `VCRMode::Record` forwards and records. `VCRMode::Replay` (the default) answers from the cassette. The cassette is one JSON file in the `vcr-cassette` format.
- How matching works: I read the source. Replay does `interaction.request == req` on the whole recorded request, which is method, URI, headers, and body together. Matching is all or nothing on the whole request. The way to loosen it is `with_modify_request`, which runs on the request in both record and replay, so you strip or rewrite volatile parts there.
- Sequence behaviour: `VCRReplaySearch::SkipFound` (default) skips a growing number of interactions from the front on each lookup, which suits strict request order. `SearchAll` searches every interaction every time, for parallel requests that arrive in random order.
- What happens on a miss: the middleware returns an error, "Cannot find corresponding request in cassette". With `with_rich_diff(true)` it logs how each recorded request differed (method, URI, headers, body).
- Streaming: record mode calls `resp.bytes().await` and stores the whole body, then rebuilds a response from it. So an SSE reply is buffered in full while recording and replayed as one body. Chunk boundaries and timing are gone.
- Record mode overwrites the existing file. It is written when the middleware is dropped.
- Secrets: `with_modify_request` and `with_modify_response` hooks. Scrubbing is entirely up to those hooks: by default the `authorization` header is written to the file as sent.
- Binary bodies: non-UTF-8 bodies are base64 encoded. Optional bzip2 zip compression of the cassette file behind the `compress` feature.
- How confirmed: fetched https://raw.githubusercontent.com/ChorusOne/rvcr/main/src/lib.rs, https://raw.githubusercontent.com/ChorusOne/rvcr/HEAD/README.md, https://crates.io/crates/rvcr.

### reqwest-vcr

- URL: https://crates.io/crates/reqwest-vcr (repository https://github.com/baptiste-le-m/reqwest-vcr/)
- Language: Rust
- License: Apache-2.0
- Version seen: 0.4.0, published 2026-03-02
- What it is: a fork of rvcr that tracks current reqwest. Feature flags pick `reqwest 0.13` with `reqwest-middleware 0.5` (default) or `reqwest 0.12` with `reqwest-middleware 0.4`.
- Matching, search modes, hooks, and compression: the README describes the same API as rvcr (`VCRMode`, `VCRReplaySearch::SearchAll`, `with_modify_request`, `with_modify_response`, bzip2 compression). I did not read its source, so I assume but did not confirm that matching is still whole-request equality.
- How confirmed: fetched https://crates.io/crates/reqwest-vcr (includes the README).

### cassetter (Rust crate)

- URL: https://crates.io/crates/cassetter and https://docs.rs/cassetter
- Language: Rust
- License: MIT. MSRV 1.88.
- Version seen: 0.17.0, published 2026-10-29. First crates.io release 2026-09-10. Very new: 54 total downloads when I looked.
- How it works: `cassetter::reqwest::Client::new(reqwest::Client::new(), recorder)` wraps a reqwest 0.13 client. A `Recorder` is built with a cassette path and a `RecordMode`. There is also a `tonic` wrapper for unary gRPC.
- Record modes: `None` (replay only, a missing or mismatched request fails), `Once`, `NewEpisodes`, `All`, `Rewrite`, `Verify`.
- Streaming: the README says plainly that the wrapper buffers request and response bodies in memory, and lists that under current limits. So SSE replays as one body here too.
- `Verify` mode is interesting for a gateway: it sends the matched request live and compares the live response to the recording. The `Compare` config has `json_ignore` paths, a float tolerance, header globs, and an `sse` option that compares an SSE body as its list of `data:` payloads.
- You must call `Recorder::finish` at the end, which saves the cassette and reports failures.
- How confirmed: fetched https://crates.io/crates/cassetter (includes the README).

### httpmock (record and playback)

- URL: https://github.com/alexliesenfeld/httpmock (README badges now point at `httpmock/httpmock`), https://httpmock.rs/
- Language: Rust
- License: MIT
- Version seen: 0.9
- How it works: this is a mock server, so the system under test points its base URL at it. `server.forward_to(target_url, ...)` forwards matching requests to the real service (crate feature `proxy`). `server.record(...)` records requests and the responses that came back (crate feature `record`). `recording.save("name")` writes a YAML file. Later, `MockServer::playback(path)` on a fresh server loads the file.
- How matching works on playback: the docs say all previously recorded requests "act as matching criteria", the same way hand-written `when` rules do. The forwarding docs add that a recording made through `forward_to` leaves the host name out of the expectations so the recording can be reused. Which request fields end up in the recorded criteria by default is something I did not confirm.
- Timing: the API docs show `rule.record_response_delays(true)` on a recording rule.
- Other: it also runs as a standalone server with a Docker image, and it can load mocks from YAML files.
- Streaming and SSE behaviour: not confirmed either way.
- How confirmed: fetched https://raw.githubusercontent.com/alexliesenfeld/httpmock/HEAD/README.md, https://httpmock.rs/, https://httpmock.rs/record-and-playback/playback/. The recording page and the docs.rs `MockServer` page are search excerpts (https://httpmock.rs/record-and-playback/recording/, https://docs.rs/httpmock/latest/httpmock/struct.MockServer.html).

### wiremock (Rust crate) and stubr

- URL: https://github.com/LukeMathWalker/wiremock-rs
- Language: Rust
- License: MIT or Apache-2.0
- What it is: an async mock server where you mount hand-written mocks (`Mock::given(...).respond_with(...)`). An unmatched request gets a 404. Expectations on call counts are checked when the server shuts down.
- Recording: the README describes mocking only. Recording is not part of what it documents. Codex (below) uses this crate with hand-built SSE bodies, which is a scripted fake, not a recording.
- stubr (https://github.com/beltram/stubr) is described in its README as "Wiremock rewritten in Rust", loading existing WireMock JSON stubs. I did not confirm whether it records.
- How confirmed: fetched https://raw.githubusercontent.com/LukeMathWalker/wiremock-rs/main/README.md and https://raw.githubusercontent.com/beltram/stubr/main/README.md.

### retake (a Rust-built proxy binary)

- URL: https://github.com/Zecrari/retake and https://zecrari.github.io/retake/docs/
- Language: the docs say it builds from source with Rust 1.88+ (`cargo install` from the git repo). It ships as a single binary for Linux, macOS, and Windows.
- License: MIT
- Age: the GitHub repo was created 2026-10-19, so this is about a month old. Version seen: v0.2.0.
- Why it matters for Rust: it is a standalone proxy, so a Rust harness can use it with zero code changes by pointing `OPENAI_BASE_URL` at it. Details are in Part 3.
- How confirmed: fetched https://zecrari.github.io/retake/docs/. Repo metadata from a search excerpt and the GitHub API.

### Goose TestProvider (in-tree, provider level)

- URL: https://github.com/block/goose (the GitHub API now reports the repo as `aaif-goose/goose`), file `crates/goose/src/providers/testprovider.rs`
- Language: Rust
- License: Apache-2.0 (GitHub API)
- Level: this records at the provider trait, above HTTP. It wraps a real `Provider` and stores the provider's typed `Message` and `ProviderUsage`, not wire bytes.
- How matching works: `hash_input(messages)` takes each message's role and content, strips internal metadata that the model never sees (`tool_meta` on tool requests, and `is_error` and `result_type` on successful tool responses), serialises with `serde_json`, and takes a SHA-256. The recording file is a JSON map from that hash to `{input, output}`. The system prompt and the tool list are stored in the record but are left out of the hash, so changing them still hits the old recording.
- Streaming: in record mode it calls the inner provider's stream, collects it into one message, and returns a single-message stream. Replay also returns a single-message stream. So streaming shape is not exercised.
- Tool-call rounds: each round has a longer message history, so each round hashes to a different key. Order does not matter; it is a pure lookup.
- What happens on a miss: `ProviderError::ExecutionError("No recorded response found for input hash: ...")`.
- The scenario test runner around it (search excerpt, at commit e94f3047): replay mode is chosen when the recording file exists. If the file is missing and `GITHUB_ACTIONS` is set, the test panics with "Test recording is not supported on CI. Did you forget to add the file ... to the repository". On a "No recorded response found" error it deletes the recording file and tells you to re-run to record fresh data.
- How confirmed: fetched https://raw.githubusercontent.com/block/goose/main/crates/goose/src/providers/testprovider.rs. The scenario runner is a search excerpt (https://github.com/block/goose/blob/e94f3047/crates/goose-cli/src/scenario_tests/scenario_runner.rs); the same path on `main` returned 404 when I tried it, so it may have moved.

### Goose ACP tests: substring match plus captured SSE, and a VCR migration that was dropped

- URL: https://github.com/block/goose/issues/6969 (opened 2026-02-04, closed 2026-04-06)
- What exists: an `OpenAiFixture` that fakes the OpenAI endpoint in-process. Each entry is a pair: a string the request body must contain, and a file of real SSE captured from OpenAI to return. Pairs are reused across tests that send the same prompt.
- Why substring matching: the issue says it was chosen over JSON equality "that would fail on any diff of the body", that it keeps checked-in files small, and that it avoids "accidentally ephemeral data". It adds that with body equality, even after scrubbing whitespace, minor differences mean rewriting the whole request file.
- The pain: capturing new data is manual. You patch the provider code to dump raw SSE to a temp file, run a compiled binary, read the request logs, and pair them up by hand.
- The proposal: move to a VCR pattern, one YAML per scenario, an env var to re-record everything, and on a failed match return HTTP 417 with a JSON diff.
- The stated cost of VCR: "a substantial amount of code" in Rust, and "a large amount of JSON blobs checked in due to repeated system prompts for every test". Also a special case: a "session rename" request that arrives in an unpredictable order.
- Outcome: the author closed it with "not pursuing this". The author notes he built the same VCR pattern in the Python version of goose and maintains it in Envoy AI Gateway, and that doing it in Rust "is tricky".
- Tip from the issue: use the exact captured SSE with no hand editing.
- How confirmed: fetched https://github.com/block/goose/issues/6969.

### Codex (Rust): wiremock plus hand-built SSE in place of recordings

- URL: https://github.com/openai/codex, file `codex-rs/core/tests/common/responses.rs`
- Language: Rust
- License: Apache-2.0 (GitHub API)
- What it does: tests start a real `wiremock::MockServer`, point the client's base URL at it, and mount SSE bodies built from small constructor functions: `sse(events)`, `ev_response_created`, `ev_assistant_message`, `ev_function_call(call_id, name, arguments)`, `ev_completed`, `sse_failed`, and more. `mount_sse_once` mounts a body with `up_to_n_times(1)` and returns a `ResponseMock` whose `single_request()` and `requests()` let the test assert on what the client sent. There are sequence and matcher variants, and a WebSocket test server that consumes a queue of request and event sequences.
- Why it is in a record and replay survey: it is the counter-example. A large Rust agent chose a scripted fake at the HTTP level over recordings. It still exercises real HTTP, serde, and the SSE parser, and the test controls tool-call ids, so the matching problem never comes up. This targets the Responses API, not chat completions.
- How confirmed: search excerpt with the full file text saved (https://github.com/openai/codex/blob/f1affbac/codex-rs/core/tests/common/responses.rs). I read the function list in that text.

## Part 3 - LLM-specific record and replay tools

These exist because the general tools above struggle with three LLM-shaped problems: SSE streams, big JSON request bodies full of volatile ids, and SDKs that retry on errors. Most of them are young and small. I list them for their design choices more than as dependencies to adopt.

### retake

- URL: https://github.com/Zecrari/retake, docs https://zecrari.github.io/retake/docs/
- Language: single binary built with Rust; language-agnostic to use
- License: MIT
- Maturity: repo created 2026-10-19, version v0.2.0. Treat as early.
- How it works: a local HTTP proxy. `retake record` followed by your test command runs that command with `OPENAI_BASE_URL` and `ANTHROPIC_BASE_URL` pointed at the proxy. `retake replay` followed by the same command serves from cassettes. Other OpenAI-compatible backends are added with an `upstream name=url` option.
- How matching works: method, upstream, path, query string, and body must be the same. JSON bodies are compared after sorting object keys, so formatting and key order never matter. Volatile fields are excluded with an `ignore` option that takes a dotted JSON path and walks arrays automatically (so `messages.id` ignores the id of every message). Headers are never stored, so they take no part in matching.
- Sequence behaviour: each recording is used once per run. Three identical calls replay the three recorded responses in recorded order. Calls that differ are matched independently, so parallel requests work regardless of timing.
- Cassette per test: the client sends an `x-retake-session` header naming the cassette. The proxy strips it before forwarding.
- Modes: `record` (each session starts empty, everything is forwarded and saved), `replay` (a new call fails with a diff), `auto` (replay what exists, forward and append what is new).
- What happens on a miss: in `replay` the call gets an HTTP 404 with error type `retake_no_match`, and the proxy prints a diff against the closest unused recording, preferring one for the same endpoint. The docs explain the status choice: the OpenAI and Anthropic SDKs automatically retry 5xx responses, which would turn a clear failure into a slow one. The whole run also exits with code 1 if any call went unmatched, even if the tests passed, so a test that swallowed the error cannot hide it.
- Streaming: SSE is passed through live while recording and stored as complete events regardless of how the bytes arrived. On replay the events are sent back byte for byte, instantly by default. Timing is opt-in: record with `record-timing` and replay with `timing realtime`. The docs say timing is off by default because it changes on every recording and makes cassette diffs noisy.
- Retries and errors: responses with status 429 or 5xx are passed through but not recorded, so SDK retries stay out of cassettes. A `record-errors` option keeps them when you want to test error handling.
- Secrets: request headers are never stored. Body strings that look like known credential formats are replaced with `[REDACTED]` before saving, and matching applies the same redaction. `retake check` scans cassettes and exits 1 on a suspected secret. The docs are honest that redaction is pattern-based and cannot recognise personal data in prompts.
- Cassette format: pretty-printed JSON, one file per session, interactions in completion order. Bodies are stored as `json`, `text`, or `base64`. Streamed responses have an `events` array where each event's `data` is the raw SSE text, with an optional `t_ms` offset.
- Replay mode sets placeholder API keys when they are missing so SDKs that require a key still start in CI.
- How confirmed: fetched https://zecrari.github.io/retake/docs/ and https://zecrari.github.io/retake/guides/vcr-for-llms/. The CI guide (https://zecrari.github.io/retake/guides/llm-tests-in-ci/) is a search excerpt.

### Docker cagent (repo now named docker-agent)

- URL: https://github.com/docker/cagent (the GitHub API returns the name `docker/docker-agent`), file `pkg/fake/proxy.go`
- Language: Go
- License: Apache-2.0 (GitHub API)
- How it works: a package described in its own header as "a VCR-based proxy for replaying recorded AI API responses". End-to-end tests start a local proxy, point the agent's models gateway at it, and replay go-vcr v4 YAML cassettes stored under `e2e/testdata/cassettes/`, named after the test.
- How matching works: a custom `DefaultMatcher`. It compares method and URL, then compares request bodies after regex normalisation applied to both the live request and the cassette. The normalisations are a good checklist of what drifts in practice:
  - tool call ids matching `call_[a-z0-9\-]+` are replaced with `call_ID` ("they change between requests")
  - `max_tokens`, `max_output_tokens`, and `maxOutputTokens` fields are removed (they vary with a model catalogue cache)
  - Gemini `thinkingConfig` and OpenAI `reasoning` config objects are removed (they vary with provider defaults)
  - the OpenAI `tool_choice` string is removed, because the client started sending it explicitly after older cassettes were recorded
  - machine-specific absolute file paths in the prompt ("Instructions from: ...") are replaced with a fixed token
- Streaming: recording uses a separate streaming-aware recorder so responses stream through to the agent in real time while being saved; the comment says the standard VCR recorder buffers entire responses. Replay can add a delay between SSE chunks: `SimulateStream` with `StreamChunkDelay`, default 15 ms.
- Secrets: when recording, the proxy injects real API keys from environment variables into the outgoing request (per provider host). A `RemoveHeadersHook` runs after capture and empties all request and response headers in the stored interaction, so keys stay out of the cassette.
- What happens on a miss: replay uses `recorder.ModeReplayOnly`. When the recorder transport returns an error, the proxy handler logs "VCR proxy request failed" and answers HTTP 500 with "Failed to run request: ...". So the agent under test sees a server error. Compare retake's choice of 404 for the same case.
- Modes seen: `ModeReplayOnly` and `ModeRecordOnce`.
- Lesson from the repo's own agent notes: "VCR cassettes include timestamps: May need regeneration if assertions check time-sensitive data".
- How confirmed: fetched https://raw.githubusercontent.com/docker/cagent/main/pkg/fake/proxy.go. The AGENTS.md notes are a search excerpt with full text saved (https://github.com/docker/cagent/blob/cb2ed7dd/AGENTS.md). The e2e helper was seen only in a fork's copy via a search excerpt.

### Envoy AI Gateway testopenai

- URL: https://github.com/envoyproxy/ai-gateway, directory `tests/internal/testopenai`
- Language: Go
- License: a source file excerpt showed an `Apache-2.0` SPDX header; I did not check repo metadata.
- Why it matters here: this is a gateway project, like one half of our system under test, and it tests against a fake OpenAI server backed by recordings.
- How it works: a test server loads all go-vcr v4 YAML cassettes embedded in the package and replays them. The README says replay uses delays "faster than real platforms to keep tests fast".
- How matching works: by cassette name first. The client sends an `X-Cassette-Name` header. Each cassette name is a Go constant with a matching typed request body kept in code (`requests.go`), and `testopenai.NewRequest(server.URL(), testopenai.CassetteChatBasic)` builds the exact request for a cassette. So the test asks for a scenario by name, and the request content is checked second.
- What happens on a miss, per the README flowchart:
  - header present, cassette found, interaction matches: return the recording
  - header present, cassette found, interaction differs: HTTP 409, "Interaction out of date"
  - header present, cassette missing, API key set: call the real API, save the cassette, return the response
  - header present, cassette missing, no API key: HTTP 500, "No cassette found"
  - header absent and nothing matches across all cassettes: HTTP 400 telling you to include the header
- Coverage seen in the cassette list (search excerpt): chat, embeddings, and legacy completions, including streaming, streaming with usage, and deliberate error cases (bad request, unknown model).
- The README points at OpenTelemetry and OpenInference instrumentation test suites as other users of VCR for LLM calls, and notes that request signing (Bedrock) and secret scrubbing differ per provider.
- How confirmed: fetched https://raw.githubusercontent.com/envoyproxy/ai-gateway/main/tests/internal/testopenai/README.md.

### The four projects named llm-vcr

The name `llm-vcr` belongs to four unrelated repos. All are small.

- yashshah9/llm-vcr - https://github.com/yashshah9/llm-vcr
  - Language: Python (PyPI name `pytest-llm-vcr`). License: MIT. Status line says v0.6.
  - How it works: an httpx transport plus a pytest decorator. YAML cassettes.
  - Matching: an exact hash by default, or `matcher="semantic"`, which ignores volatile keys, treats dated model aliases as equal (a trailing `-YYYY-MM-DD` on the model name is normalised away), compares messages by role and content, and compares tools by name. For Anthropic it ignores `metadata` and the `tool_use` and `tool_result` ids.
  - Tool-call rounds: sequential matching is opt-in with `sequential=True`.
  - Streaming: cassettes store `streaming: true` plus a `chunks` list. The limits section says it stores chunks, not per-event timestamps.
  - It ships an `llm-vcr diff` command that shows normalised differences between two request bodies.
  - How confirmed: fetched https://raw.githubusercontent.com/yashshah9/llm-vcr/HEAD/README.md.
- BhagyashriT/llm-vcr - https://github.com/BhagyashriT/llm-vcr
  - Language: Python, standard library only. License: MIT (GitHub API).
  - How it works: a proxy in front of any OpenAI-compatible endpoint. One JSON file per interaction, named by hash.
  - Matching: SHA-256 of the normalised request - model, messages, and other params as canonical JSON. Normalisation ignores transport keys `stream` and `stream_options`.
  - Streaming: streaming requests are rejected with HTTP 400; cassettes hold complete interactions only.
  - Extra: a `regress` command replays every cassette against a target and diffs the answers by meaning.
  - How confirmed: search excerpt only.
- Mhamadkadiry/llm-vcr - https://github.com/Mhamadkadiry/llm-vcr
  - Language: TypeScript, for Vitest and Jest. License: the GitHub API shows none detected.
  - Cassette files are named by a hash (the README example is `a3f9b2c1.json`). It says SSE streams are recorded chunk by chunk and replayed instantly, and that `Authorization` and `x-api-key` headers are always redacted. Suggested setup: `mode: process.env.CI ? 'replay' : 'record'`.
  - How confirmed: search excerpt only.
- SiluPanda/llm-vcr - https://github.com/SiluPanda/llm-vcr
  - Language: TypeScript, Node 18+, zero runtime dependencies. License: MIT (GitHub API).
  - How it works: patches `globalThis.fetch`. Non-LLM requests pass through.
  - Matching: "provider-aware" - by model, messages, tools, temperature, and other LLM fields instead of raw HTTP bodies. Ordered matching (declaration order) or unordered (best match by score).
  - Modes: `auto`, `record`, `replay`, `passthrough`. In `replay` a miss throws `CassetteMismatchError`.
  - How confirmed: search excerpt with full README text saved; I read the feature list.

### cassette (faisalkhan91)

- URL: https://github.com/faisalkhan91/Cassette
- Language: Go library plus a CLI and a reverse proxy mode usable from any language
- License: MIT
- How it works: a custom `http.RoundTripper` injected into the official SDK's HTTP client. It records below the SDK's retry layer. `cassette proxy` and `cassette up` run it as a reverse proxy.
- How matching works: the README says the match key is "path-only" for provider detection and that request nondeterminism is normalised away: reordered JSON keys, gzip versus identity encoding, and volatile headers (api-key, request-id, idempotency-key, timestamps) never change the match. I did not read the matching code, so the exact key is unconfirmed.
- Streaming: SSE is captured at the byte level and replayed verbatim, including original frame boundaries, ping and comment lines, and terminal events. It reassembles streamed tool calls from fragmented argument deltas, including splits in the middle of a multibyte UTF-8 character.
- Two kinds of "same": WIRE equivalence (replay equals the cassette bytes, chunk boundaries included) and SEMANTIC equivalence (the assembled message - role, text, tool calls with arguments parsed and canonicalised, stop reason - is equal after response-side volatile fields such as ids, timestamps, and usage are normalised out). This split is a useful vocabulary for our own assertions.
- What happens on a miss: in replay the base transport blocks all dials, so a miss is a hard error. A dial counter lets the test assert that zero network connections were attempted. The test helper also asserts every recorded interaction was consumed.
- Modes: default is replay. `CASSETTE_MODE=record|replay|auto`, or `go test -update` to record.
- Also records MCP stdio JSON-RPC at the client call boundary.
- The README is careful to claim byte-identical transport replay, and explicitly does not claim that live models are deterministic.
- How confirmed: fetched https://raw.githubusercontent.com/faisalkhan91/Cassette/HEAD/README.md.

### llm-cassette (thbabs)

- URL: https://github.com/thbabs/llm-cassette
- Language: Python
- License: MIT (GitHub API)
- Version seen: 0.1.0
- Level: SDK call level, above HTTP. It patches `chat.completions.create` and `responses.create` on the OpenAI Python SDK, and `messages.create` and `messages.stream` on the Anthropic SDK. It also records plain Python tool functions with a `@tool` decorator.
- Cassette format: JSONL, one line per `llm_call` or `tool_call`.
- How matching works: replay walks the file in order and matches a hash of the normalised request. Before hashing, volatile fields (`id`, timestamps, request ids) are removed and secret fields are replaced with a placeholder. A custom `matcher` can replace the comparison. On a miss the error shows the expected and the received normalised requests.
- A sharp edge the README admits: volatile-field removal works by key name, so a request field named `id` is dropped wherever it sits, including an OpenAI tool call's `id`, while `tool_call_id` is kept.
- Tool-call rounds: tool calls are recorded as their own events, and on replay the tool function is not called; its recorded result is returned.
- Streaming: replay re-emits streams as the SDK's own stream objects.
- Modes: `once`, `replay` (a missing file raises `CassetteNotFoundError`), `record`, `none`.
- Other limits it lists: one cassette is not safe for concurrent use; a per-request timeout is deliberately left out of the recorded request so changing it does not break the match; the pytest plugin leaves `strict` off, so a test can pass without replaying every recorded interaction.
- How confirmed: fetched https://raw.githubusercontent.com/thbabs/llm-cassette/HEAD/README.md.

### vcr-langchain

- URL: https://github.com/amosjyng/vcr-langchain
- Language: Python
- License: MIT (GitHub API). Last push 2024-06-26, so it looks dormant.
- What it is: a patch on top of VCR.py that also records non-network LangChain tooling, so that on replay "no command executions or other side effects actually happen". It can swap real Playwright browsers for fake ones on replay.
- Pitfall the README states: tools initialised outside the decorator are not patched, even if the agent using them is initialised inside it.
- The README also says a lot of LangChain functionality is not yet covered.
- How confirmed: fetched https://raw.githubusercontent.com/amosjyng/vcr-langchain/HEAD/README.md.

### baml_vcr

- URL: https://github.com/gr-b/baml_vcr
- Language: Python
- License: the README says MIT; the GitHub API shows none detected. Not on PyPI. Last push 2025-07-30.
- What it is: a third-party tool, not part of BAML itself. It patches BAML client functions and records at the function level: function name, arguments, typed response, usage. YAML cassettes named after the test class and method.
- How matching works: by function name and arguments. The troubleshooting section says to "check that the function arguments match exactly".
- Record modes: `once`, `new_episodes`, `none` ("use in CI/CD pipelines"), `all`.
- Streaming: chunks are recorded one by one and replayed "with realistic timing". Streaming cassettes use a separate file extension, and the README warns against mixing streaming and non-streaming calls in one test.
- BAML itself: my search surfaced only BAML's `Collector`, which does token and provider prompt-cache accounting. A record and replay cache built into BAML stays unconfirmed.
- How confirmed: fetched https://raw.githubusercontent.com/gr-b/baml_vcr/HEAD/README.md.

### Projects named agent-vcr

Again several unrelated repos share the name.

- Jarvis2021/agent-vcr - https://github.com/jarvis2021/agent-vcr
  - Language: Python and TypeScript. License: MIT.
  - Scope: MCP, not LLM calls. It records JSON-RPC 2.0 traffic between an MCP client and server into `.vcr` files and replays them as a mock server over stdio or SSE. It has a `diff` command to catch contract changes between two recordings.
  - Matching strategies named in its architecture doc (search excerpt): exact JSON equality excluding `jsonrpc` and `id`, method only, method and params, subset, and sequential order.
  - How confirmed: search excerpt with full README text saved.
- ajayvnkt/agent-vcr - https://github.com/ajayvnkt/agent-vcr
  - Language: TypeScript. License: MIT (GitHub API).
  - Scope: it records the sequence of tool calls an agent makes with a real model once, saves it as a "golden trace", and in CI a `ScriptedLlm` replays assistant turns while `compareTraces` checks the tool sequence. Its README says it tests tool dispatch, routing, and argument shapes, and does not test model quality.
  - How confirmed: search excerpt only.
- ixchio/agent-vcr - https://github.com/ixchio/agent-vcr - appeared in search results with a pytest fixture and an `AGENT_VCR_RECORD=none` CI mode. Not examined.

## Part 4 - How real projects use cassettes in their LLM tests

### Pydantic AI

- URL: https://github.com/pydantic/pydantic-ai, guide at `tests/AGENTS.md`
- Language: Python
- License: MIT (GitHub API)
- Policy: "VCR + public-API tests are the default." Tests go through the public API (`Agent(...)`, `agent.run(...)`) against real provider responses recorded as cassettes. Their reasoning: provider APIs are the ultimate judge of whether the code is correct.
- Workflow: record with `record-mode=rewrite`, verify playback without the flag, review the diff. API key fixtures default to `'mock-api-key'` and real keys load from the environment only when recording.
- Hard rules in the guide: every cassette must be recorded by its own test. Never copy a cassette from another test, and never edit its requests or responses by hand, because replay would then test the code against an exchange that no provider produced.
- The weakness they name themselves: "our cassette matchers aren't always sensitive to the request body, so a changed internal payload can still match an existing recording and pass green". Their answers, in order of preference:
  - Tap the boundary: an httpx request hook fixture (`request_capture`) that fires on replay too, so assertions run against the request the live code actually built, not the frozen recording.
  - Snapshot the request body from the cassette, as a review surface. The guide is explicit that this does not catch drift, because the recording is frozen.
  - Add a per-test matcher (`additional_matchers=['body']`, or `json_body` with `ignore_json_paths`) so a drifted request fails to match. Match only on fields the test's claim rests on, since matching a field that legitimately varies breaks replay.
- Another rule: "Pin recorded facts in the test body, never in the cassette alone", because cassettes are long and humans skim them.
- Cost of recording: the guide says recording needs provider keys and "isn't trivial, so contributors routinely under-test the real behavior", and calls writing those tests core maintainer work.
- Cassettes also serve as a regression net for prompt-cache prefixes: a check that consecutive requests extend the same serialised prefix.
- Beyond HTTP: realtime WebSocket cassettes hold raw frames with secrets scrubbed and audio truncated. For the xAI SDK, which uses gRPC, they wrote custom protobuf cassettes because VCR records only HTTP (search excerpt).
- The measured drift (issue 6993, opened 2026-07-31, still open): VCR was configured to match on method, scheme, host, port, path, and query, never the body. Adding `body` to the matcher made 435 tests fail out of roughly 9,100 that ran (435 failed, 8658 passed). The issue calls 435 an upper bound, since some bodies legitimately vary. In one directory, 29 of 30 cassettes were truly stale, recorded before a second tool was added. Re-recording them turned two cases red, and those two were a real production bug that nothing had caught, "because the recording still contained the pre-bug wire and the assertions were reading the recording".
- A maintainer's reply in that issue: `body` matching is "too coarse" and he had tried and failed at it before; the direction they converged on is asserting or snapshotting specific payload fields.
- History: pytest-recording was added in PR 969 (merged 2025-02-23). The PR text says it makes it much easier to turn user-reported issues into tests, and that it does not remove the value of the scripted `FunctionModel` and `TestModel`, which are still needed for dynamic logic (search excerpt).
- A Pydantic article says the repo has "thousands of lines of recorded cassettes" and that the downside is you need to invalidate them at some point or you rely on old data (search excerpt).
- How confirmed: fetched https://raw.githubusercontent.com/pydantic/pydantic-ai/main/tests/AGENTS.md and https://github.com/pydantic/pydantic-ai/issues/6993. PR 969, the xAI cassette file, and the article are search excerpts (https://github.com/pydantic/pydantic-ai/pull/969, https://pydantic.dev/articles/building-agentic-application.md).

### Simon Willison's `llm` tool and plugins

- URL: https://til.simonwillison.net/pytest/pytest-recording-vcr and https://github.com/simonw/llm
- Language: Python
- License: Apache-2.0 for `simonw/llm` (GitHub API)
- Pattern: `@pytest.mark.vcr` on tests, a module-scoped `vcr_config` fixture returning `{"filter_headers": ["authorization"]}`, and a key that is either `PYTEST_OPENAI_API_KEY` from the environment or the literal `"fake-key"`. To refresh: set the env var and run with `record-mode once`.
- He confirmed replay makes no network requests by turning off his laptop's WiFi.
- The `llm-openai-plugin` README pairs pytest-recording with syrupy snapshots, recording and updating snapshots in one command, then reviewing the new snapshot files (search excerpt).
- How confirmed: fetched https://til.simonwillison.net/pytest/pytest-recording-vcr. The plugin README is a search excerpt (https://pypi.org/project/llm-openai-plugin/).

### LangChain standard tests

- URL: https://github.com/langchain-ai/langchain, `libs/standard-tests/langchain_tests/conftest.py`
- Language: Python
- License: MIT (GitHub API)
- Base VCR config they ship: `record_mode: once`, `match_on: ["method", "uri", "body"]`, `allow_playback_repeats: True`, `decode_compressed_response: True`, and header filters that replace `authorization`, `x-api-key`, and `api-key` with a placeholder. Note that body is part of the match here.
- Cassette size: they ship a custom serializer that writes cassettes as gzip-compressed YAML (`.yaml.gz`), built on `yaml.safe_dump`. The docs warn that compression makes cassettes harder to inspect and show how to gunzip one to check for secrets.
- The docs warn that VCR by default records authentication headers.
- CI rule, from the script notes for one integration: CI runs `pytest record-mode=none -m vcr`, and "a missing or stale cassette is a hard failure - exactly the signal you want".
- Secret scan after recording: that same script greps every cassette for bearer tokens, JWT-shaped strings, refresh-grant bodies, and leaked API keys, and aborts with a non-zero exit on any hit. Its conftest redacts every request and response header, request URIs, OAuth fields in JSON and form bodies, and any JWT-shaped string in a body.
- The LangChain integration testing docs add the drift warning: when you modify prompts, add tools, or change expected trajectories, saved cassettes become outdated and existing tests fail.
- How confirmed: all search excerpts with long page text (https://github.com/langchain-ai/langchain/blob/cdafe607af6ba4c0ae8c73575488f74ce7962da1/libs/standard-tests/langchain_tests/conftest.py, https://reference.langchain.com/python/langchain-tests/integration_tests/chat_models/ChatModelIntegrationTests, https://github.com/langchain-ai/langchain/blob/98216c0c/libs/partners/openai/scripts/RECORD_CODEX_CASSETTES.md, https://docs.langchain.com/oss/python/langchain/test/integration-testing).

### LangSmith test cache

- URL: https://docs.langchain.com/langsmith/pytest
- What it is: the LangSmith pytest plugin can cache HTTP requests to disk. Set `LANGSMITH_TEST_CACHE=tests/cassettes`. The docs suggest checking the directory in so CI uses the cache too. `@pytest.mark.langsmith(cached_hosts=[...])` limits it to chosen hosts.
- Under the hood (SDK source): it is VCR.py with `record_mode="new_episodes"`, `match_on=["uri", "method", "path", "body"]`, request headers emptied, and `authorization` and `Set-Cookie` filtered. A code comment says other modes are a to-do.
- Consequence: because it is `new_episodes`, a changed prompt silently records a new call. That is a cache, not a guard: a miss gets recorded and the test stays green.
- How confirmed: search excerpts (https://docs.langchain.com/langsmith/pytest, https://langsmith-sdk.readthedocs.io/en/stable/_modules/langsmith/utils.html).

### NVIDIA NeMo Guardrails recorded tests

- URL: https://github.com/NVIDIA-NeMo/Guardrails, `tests/recorded/README.md`
- Language: Python (pytest-recording)
- This is the most carefully reasoned cassette policy I found. Key points:
  - Replay runs with `block-network`, and "a cassette miss with `block-network` is a test failure". Replay installs dummy API keys.
  - What goes where: tests that send at least one real provider request live in the recorded suite. Synthetic client and wire conditions - status code to exception mapping, retries, SSE framing, malformed bodies - live in a separate suite that uses a mock transport plus JSON fixtures. Local validation and rail decisions "do not need fabricated provider recordings".
  - For error paths, prefer a recordable real error (a nonexistent model name gives a real, deterministic 404) over a fake.
  - Hand-made "fake cassettes" are a last resort. They must sit in a `fake/` directory, use a marker, be excluded from refresh, and include metadata with a `reason` and `frozen_fields`.
  - Volatile response fields (ids, timestamps, fingerprints) are scrubbed to fixed sentinels in the cassette, so snapshots can assert on them directly.
  - Response headers are dropped by exact name and by prefix (`x-`, `cf-`, `openai-`), with an allow list.
  - JSON bodies are stored as `parsed_body` so the YAML stays readable; SSE responses are stored as parseable events.
  - Tool-call streaming: the cassettes keep the provider's real tool-call ids. Only the snapshots normalise volatile ids. Replay reconstructs the calls from recorded fragments and compares ids, names, and arguments with what the public API emitted.
  - The in-process ASGI request is excluded from VCR, and each test requires exactly one provider exchange "so replay cannot bypass the application". Assertions also require all recorded exchanges to be consumed.
  - They state the limit of what a recording proves: it checks assembled SSE responses, "not socket timing". Timing, interruption, and cancellation are covered by separate gated-provider tests.
  - Refresh is a make target that records, updates inline snapshots, then verifies offline. It runs only in a trusted environment with real credentials, and changed provider payloads get reviewed before snapshots are updated.
  - A recording oddity they note: one parameter combination timed out during recording, so the cassettes do not cover it.
- How confirmed: search excerpt with the full README text saved, which I read end to end (https://github.com/NVIDIA-NeMo/Guardrails/blob/develop/tests/recorded/README.md).

### Mirascope, Instructor, LlamaIndex

- Mirascope: a GitHub code search for `pytest.mark.vcr` in `Mirascope/mirascope` returned 48 hits, all under `python/tests/e2e/`. So it uses VCR-style cassettes for end-to-end tests. I did not read its config.
- Instructor (`567-labs/instructor`) and LlamaIndex (`run-llama/llama_index`): the same code search returned zero hits for `pytest.mark.vcr` and for `vcr`. That is weak evidence (code search indexes can be incomplete), so their use of cassettes stays unconfirmed.
- How confirmed: GitHub code search on 2026-11-22.

### SDK test suites that use a scripted fake or a spec mock instead

- OpenAI Agents SDK (https://github.com/openai/openai-agents-python, `tests/fake_model.py`, MIT): a `FakeModel` holds a queue of per-turn outputs. `set_next_output` and `add_multiple_turn_outputs` push items or exceptions; each model call pops the next one. Tool-call rounds are scripted as consecutive turns. It also keeps the first and last call arguments so tests can assert on what the agent sent. This is a scripted model, the same idea as our current one. Search excerpt.
- Anthropic Python SDK (https://github.com/anthropics/anthropic-sdk-python): CONTRIBUTING says most tests run against a mock server generated from the OpenAPI spec (`./scripts/mock`, which currently starts a tool called Steady on port 4010). Some tests "capture snapshots of the HTTP requests they make", refreshed with an `http-record` flag. Search excerpt.
- Aider: a GitHub code search for `cassette OR vcr` under `tests` in `Aider-AI/aider` returned zero hits. Unconfirmed either way.

## Part 5 - Cache layers (a cousin of record and replay)

A cache sits inside the client library and answers a repeated call from storage. It is built to save money and time, and its default on a miss is to call the real model. That is the opposite of what a test wants in CI. The useful part for us is how each one builds its key.

### LangChain LLM cache (`InMemoryCache`, `SQLiteCache`)

- URL: https://github.com/langchain-ai/langchain (`libs/core/langchain_core/caches.py`) and `langchain_community/cache.py`
- Language: Python. License: MIT (GitHub API).
- Key: the pair `(prompt, llm_string)`. `prompt` is a string serialisation of the prompt (for chat models, a serialisation of the messages). `llm_string` is a string serialisation of the LLM configuration: model name, temperature, stop tokens, max tokens, and so on.
- `SQLiteCache` is a SQLAlchemy table with `prompt`, `llm`, and `idx` as the primary key and the serialised generation as the value. Default file `.langchain.db`.
- On a miss: `lookup` returns `None` and the model is called, then `update` stores the result.
- Enabled globally with `set_llm_cache(...)`.
- How confirmed: search excerpts with source text (https://github.com/langchain-ai/langchain/blob/master/libs/core/langchain_core/caches.py, https://github.com/langchain-ai/langchain-community/blob/b0cd3323a31959f57c55f19e0422635619cee095/libs/community/langchain_community/cache.py).

### GPTCache

- URL: https://github.com/zilliztech/GPTCache
- Language: Python. License: MIT (README badge).
- What it is: a semantic cache. It embeds the query, searches a vector store for similar past queries, and a similarity evaluator decides whether it is a hit. An exact-match mode exists too.
- The README itself warns that a semantic cache produces false positives on hits and false negatives on misses. That makes it a poor fit for deterministic tests: a near-match prompt can return the wrong recording. It also needs an embedding model, which breaks the "zero inference" goal unless you use exact match.
- How confirmed: fetched https://raw.githubusercontent.com/zilliztech/GPTCache/main/README.md.

### LiteLLM cache

- URL: https://docs.litellm.ai/docs/caching/all_caches
- Language: Python. License: the GitHub API reports no single detected SPDX id for the repo.
- Backends: in-memory, disk, Redis, S3, GCS, and semantic variants (Redis, Valkey, Qdrant) with a `similarity_threshold`.
- Key: built by `litellm.cache.get_cache_key`, which you can replace. The docs' custom example concatenates `model`, `messages`, `temperature`, and `logit_bias`. I did not confirm the exact default field list.
- Per-call control: `caching=True` on a call, a `ttl`, and a list of supported call types (`completion`, `acompletion`, `embedding`, and so on).
- How confirmed: fetched https://docs.litellm.ai/docs/caching/all_caches.

### promptfoo cache

- URL: https://www.promptfoo.dev/docs/configuration/caching/
- Language: TypeScript. License: MIT (GitHub API).
- Key: provider-specific composite keys that include the provider identifier, the prompt or request content (often as a deterministic digest), the provider configuration, and context variables. A second layer caches raw HTTP fetches keyed on URL, method, headers, options, and body. The docs call the key formats implementation details that may change.
- Behaviour: successful responses are cached; error responses and empty responses are not. Default TTL is 14 days. Disk storage under `~/.promptfoo/cache` by default, and memory storage automatically when `NODE_ENV=test`.
- Repeats: with a repeat count above 1, each repeat index gets its own cache namespace, so repeat 0 and repeat 1 keep distinct outputs across runs.
- For CI with an external test framework the docs suggest setting `PROMPTFOO_CACHE_TYPE=disk` and a cache path.
- How confirmed: fetched https://www.promptfoo.dev/docs/configuration/caching/.

### Inspect AI: cache and `mockllm`

- URL: https://inspect.aisi.org.uk/caching.html and https://github.com/UKGovernmentBEIS/inspect_ai
- Language: Python. License: MIT (GitHub API).
- Cache key, as documented: model name and base URL; model constructor arguments; the model prompt (message history); the epoch number; the generate config (temperature, top_p, and so on); and the active `tools` and `tool_choice`. If all are identical the response comes from the cache.
- Details worth copying: the key leaves out `api_key`. The epoch number is in the key so repeated runs of the same sample get distinct generations. Default expiry is one week, set with a `CachePolicy`. Named `scopes` can be added to the key.
- A caveat they document: the model name is part of the key, but many names are aliases that resolve to different versions over time. Use an explicitly versioned model name if you want a new version to miss the cache.
- A warning about key changes: caches written by versions that did not include constructor arguments in the key can return an answer written for different arguments, so they tell you to clear the cache after upgrading. That is cassette drift in cache form: when the key definition changes, old entries become wrong, not just stale.
- `mockllm`: a built-in provider, `get_model("mockllm/model", custom_outputs=[...])`. `custom_outputs` takes a list, a generator, or a callable that receives the messages and returns a `ModelOutput`. `ModelOutput.for_tool_call(model, tool_name, tool_arguments)` scripts a tool call. This is a scripted fake with an escape hatch for input-dependent replies.
- How confirmed: fetched https://inspect.aisi.org.uk/caching.html. `mockllm` is from search excerpts, including an example file in the Inspect repo (https://github.com/UKGovernmentBEIS/inspect_ai/blob/main/examples/inline_cards/question.py).

### DSPy cache

- URL: https://dspy.ai/tutorials/cache/ (source at `docs/docs/tutorials/cache/index.md` in https://github.com/stanfordnlp/dspy)
- Language: Python. License: MIT (GitHub API).
- Layers: an in-memory LRU cache, an on-disk cache (`diskcache.FanoutCache`), and the provider's own server-side prompt cache, which DSPy does not control.
- Key: the docs say LM cache keys include the canonical request and DSPy's execution identity (such as `rollout_id`), excluding credentials like `api_key`. Sync and async calls have separate entries.
- Custom key example in the docs: subclass the cache and return `sha256(orjson.dumps(messages, option=orjson.OPT_SORT_KEYS))`, so the key depends only on the message content and ignores which model is called.
- A tell-tale of a hit: usage metrics for a cached call are empty.
- Safety: the disk cache uses pickle, with a restricted set of types allowed by default.
- How confirmed: fetched https://raw.githubusercontent.com/stanfordnlp/dspy/main/docs/docs/tutorials/cache/index.md.

## Part 6 - Agent trajectory replay

This is replay one level up: instead of replaying model HTTP traffic, replay the agent's recorded actions or decisions.

### SWE-agent `run-replay`

- URL: https://swe-agent.com/latest/usage/cli/
- Language: Python. License: MIT (GitHub API).
- What it does: `sweagent run-replay` takes all actions from a trajectory (`.traj`) or demo file and executes them again in the environment. The docs describe it as useful for debugging tools or building new demonstrations. The model is not called; the environment is real.
- The docs page also shows a notice that SWE-agent is in maintenance-only mode and has been superseded by mini-swe-agent.
- How confirmed: search excerpt of the CLI docs page (https://swe-agent.com/latest/usage/cli/) and the demonstrations page (https://swe-agent.com/latest/config/demonstrations/).

### OpenHands trajectory replay

- URL: https://docs.openhands.dev/openhands/usage/v0/advanced/V0_configuration-options and `openhands/controller/replay.py`
- Language: Python. License: MIT (GitHub API).
- What it does: in the legacy V0 code, `save_trajectory_path` writes a JSON trajectory and `replay_trajectory_path` replays its actions before any new user instruction. The pull request demo showed replay working with a wrong LLM key, because the model is not called.
- The honest warning in the source: unexpected or erroneous results can happen if any action is non-deterministic, or if the initial state before replay differs from the initial state of the trajectory. Environment events and observations are skipped; only agent and user actions are replayed.
- Status: the replay file is marked legacy V0, deprecated since 1.0.0. The V1 design notes say the conversation state is the single mutable source of truth, which "enables deterministic replay".
- How confirmed: search excerpts (https://github.com/All-Hands-AI/OpenHands/blob/3ec999e8/openhands/controller/replay.py, https://github.com/All-Hands-AI/OpenHands/pull/6215, https://docs.openhands.dev/sdk/arch/design).

### Temporal-style deterministic replay

- URL: https://docs.temporal.io/develop/python/best-practices/testing-suite and https://go.temporal.io/platform-hub/ai-engineering/ai-reference-architecture
- The model: orchestration lives in deterministic Workflow code. Every LLM call, tool call, and other I/O lives in an Activity. A completed Activity's result is written to the Event History, and on replay the result is read from history instead of calling the LLM again.
- For tests: download representative Event Histories, run them through a `Replayer` against the current workflow code, and fail CI on any non-determinism error. This checks that the orchestration code still makes the same sequence of calls; it says nothing about model quality.
- Their caution about fixtures: production histories may hold encrypted payloads or personal data, so scrub them or do not use them for tests.
- Relevance: this is "sequence order per run" as the replay key. The recorded history is the script, and any change in the order or kind of calls is a hard failure.
- How confirmed: search excerpts of the Temporal docs pages.

### Decision-level replay versus wire-level replay

- Source: https://dreaming.press/posts/record-replay-testing-for-ai-agents.html (an article dated 2026-06-27 whose byline names an AI model as author; treat it as an opinion piece, though its claim about cagent matches the source code I read)
- The distinction it draws: an HTTP cassette freezes the model output and, in setups where tools are also reached over the recorded wire, every tool output too, so tool code never runs on replay and a bug in a tool stays green. A decision-level recording stores only the model's choices (which tool, which arguments, what text) and re-executes the real tools, so tool regressions fail the test, but provider or serialisation drift goes unseen.
- It names a project called `langchain-replay` for the decision-level approach. I did not open that project, so it is unverified here.
- Relevance to us: in a harness where tools run locally and only the model call crosses HTTP, a chat-completions cassette freezes only the model's turns. The tools still run for real, and their real output goes into the next request, which is exactly what makes later requests fail to match if a tool's output changes.
- How confirmed: fetched the article.

## Part 7 - Write-ups on the pitfalls

### retake guide: "VCR for LLMs: record/replay vs mocks vs live calls"

- URL: https://zecrari.github.io/retake/guides/vcr-for-llms/ (updated October 2026; vendor content, so read it as a pitch with good detail)
- Its comparison of hand-written mocks, live calls, and recordings:
  - Uses real model output: mocks no, live yes, recordings yes as of when recorded
  - Deterministic: mocks yes, live no, recordings yes
  - Needs a key in CI: mocks no, live yes, recordings no
  - Effort for multi-step agents: mocks high, live nothing, recordings one recording run
  - Notices model changes: mocks no, live yes, recordings only when re-recorded
- Why LLM APIs are a special case, per the guide: streaming (replay must reproduce the event stream, not just a final JSON body); huge nested JSON requests (matching must ignore formatting and key order, and a failed match needs a readable diff, not just "no cassette found"); secrets in headers and sometimes in prompts; SDK retries on 429 and 5xx (a recorder that saves those, or a replayer that fails with a 500, produces confusing slow failures); and many languages in one team.
- How to choose, per the guide: a small mock for testing a parser, validator, or prompt template in isolation; record and replay for code that depends on what the model actually says (an agent loop, a RAG pipeline, structured extraction); live calls on a schedule for evals of output quality.
- Its CI guide (search excerpt) adds: replay strictly in CI with no key, so fork pull requests can run the full suite; and run a scheduled weekly job with a real key that re-records and shows the cassette diff, to catch model drift.
- How confirmed: fetched https://zecrari.github.io/retake/guides/vcr-for-llms/. CI guide is a search excerpt (https://zecrari.github.io/retake/guides/llm-tests-in-ci/).

### evalship: "Recorded LLM replies in Rails tests pass any prompt change"

- URL: https://evalship.com/articles/recorded-llm-replies-in-rails-tests (dated 2026-11-06; vendor content with a stated method)
- The survey: they read the tests of 39 open-source Ruby apps that call an LLM. 19 replay the model's HTTP response. In 16 of those 19 the replay matches without looking at the request, so the test passes whatever prompt or model the code sends. 3 replay only when the request body matches. 14 stub the client object and skip HTTP entirely. 4 left the AI call untested.
- The example: a personal-finance app whose test replays a cassette recorded in April 2025 with `gpt-4.1-mini` and one rule in the prompt. The code now sends `gpt-4.1` and the opposite rule. The test still passes. "It would also pass with an empty prompt."
- The cause: VCR matches on method and URI unless told more, and every call to the same LLM endpoint is a POST to the same URL. The article notes that default is reasonable for ordinary APIs where the body is noise, but for an LLM the body is the prompt, the model, and the settings.
- What the replays still check: that the app builds a request, parses the reply, and handles errors.
- The fix they suggest: match on method, URI, and body, and never record on CI (`record: ENV["CI"] ? :none : :once`). Then a changed prompt raises an unhandled-request error until someone re-records with a real key.
- The limit of even that fix: it tells you the request changed, not whether the new reply is still right. A re-recorded cassette holds whatever the model said that day.
- A trap in stubbed tests: a few apps build the expected request body from the same constant the code uses, so a prompt edit changes both sides and still passes.
- How confirmed: fetched the article.

### Privorum: "Record and replay for LLM tests"

- URL: https://privorum.com/insights/record-and-replay-for-llm-tests/ (dated 2026-09-19)
- Why they left hand-written mocks: their mock returned a neutral, middle-of-the-road answer, and that hid real parsing bugs because real responses have a different shape.
- How they run it: replay is the default and recording is a deliberate act; the replay suite is a required merge check; auth headers, signatures, and nonces are scrubbed before saving, with a pipeline check that fails if a cassette still holds a secret; and they match on method, URL, and a per-scenario tape "so that a prompt tweak does not invalidate every recording".
- What went wrong: they found ten defects in the recording path before recording a single paid tape. Two they describe: a recording with zero interactions overwrote a good tape (a failed re-record wiped a working cassette), and a long scenario name was truncated so two scenarios shared one tape.
- Their conclusion: "Treat the recorder as code that needs tests. A silent recorder bug turns your whole suite into a false green."
- The honest limit they state: replay proves your code handles a specific response. It does not prove today's model still answers that way. Re-record on a schedule or before a model change, and keep a small live smoke test.
- Note the tension with evalship: Privorum deliberately matches loosely (scenario tape, not body) to avoid invalidating recordings on a prompt tweak. evalship counts that exact setup as a test that cannot fail on a prompt change. Both are right about what they optimise for.
- How confirmed: fetched the article.

### Multigrid: "Mocking and Recording LLM Responses in Tests"

- URL: https://multigrid.ai/learn/vcr-for-llm
- Confirmation status: the direct fetch failed three times with HTTP 525. Everything below is from two long search excerpts of the page, so treat it as second-hand.
- The division of labour it proposes: a hand-written fake is better than a cassette for testing logic (it is explicit, readable, and does not drift). A cassette is better for exactly one thing, fidelity: the usage field layout, the ordering of streamed chunks, how a tool call is split across deltas, an unusual finish reason. Scripted fakes for behaviour (retries, deadlines, breakers, business rules); cassettes for the boundary (adapter parsing, streaming assembly, usage accounting).
- On keys: "The matcher is the design." Match on too much and every trivial edit misses; match on too little and a test gets someone else's recording and passes for the wrong reason. Its suggested key is a SHA-256 of canonical JSON holding the endpoint path, the model, the messages, and the tools, and it excludes auth headers, user-agent, request id, idempotency key, trace headers, and timestamps.
- On volatile prompts: if the prompt contains a timestamp, a UUID, a random sample, or a locale-dependent format, the key changes every run. Its advice is to make the clock and random source injectable and fix them in tests, not to relax the matcher.
- On streams: record the chunk sequence with relative timings (`at_ms`), and make timing a flag on the replayer: with delays to test a time-to-first-token timeout or an idle detector, without delays for the ordinary suite. Record awkward cases on purpose: a chunk that splits a multi-byte character, a tool call whose arguments arrive across several deltas, a stream that ends without a terminal event.
- On rot: three modes (replay by default where a missing cassette fails; record-new for development; re-record-all on a branch so the cassette diff is reviewed). Stamp each cassette with the date and model id, warn or fail when a cassette is older than a threshold, and run the re-record job on a schedule with a separate budgeted key so the diff arrives as a pull request.

## Part 8 - How replay keys are built in practice

Ordered from loosest to strictest, with who does it.

- Scenario or cassette name only, chosen by the test:
  - Envoy AI Gateway picks the cassette by an `X-Cassette-Name` header, then checks the interaction.
  - retake picks the cassette by an `x-retake-session` header, then matches inside it.
  - pytest-recording and most others name one cassette per test, which narrows the search space the same way.
  - Privorum matches on method, URL, and a per-scenario tape.
- Method plus URL (body ignored):
  - VCR.py default, Ruby VCR default, cassetter default, and Pydantic AI's suite config.
  - For chat completions this means "any recording matches any prompt", and correctness rests on call order inside the cassette.
- A substring of the request body:
  - Goose ACP tests: the body must contain a given string, such as the user's question.
- Sequence order within a test:
  - Polly.js `order: true`; VCR.py's play-each-recording-once; cassetter "at most once, in order"; retake for identical calls; llm-cassette walking its JSONL in order; WireMock scenarios; Hoverfly stateful capture; rvcr `SkipFound`; Temporal event history; the OpenAI Agents SDK `FakeModel` queue.
- Hash of the conversation only:
  - Goose TestProvider: SHA-256 of each message's role and content with internal metadata stripped. System prompt and tool list are left out of the hash.
  - DSPy's custom-cache example: SHA-256 of the messages with sorted keys.
- Selected request fields:
  - Multigrid's proposal: endpoint path, model, messages, tools.
  - BhagyashriT/llm-vcr: model, messages, and other params as canonical JSON, with `stream` and `stream_options` ignored.
  - SiluPanda/llm-vcr: model, messages, tools, temperature, and other LLM fields.
  - Inspect AI cache: model name and base URL, constructor args, messages, epoch, generate config, tools and tool_choice.
  - LangChain cache: prompt string plus an LLM config string.
  - baml_vcr: function name plus arguments.
- Normalised meaning of the request:
  - yashshah9/llm-vcr semantic matcher: dated model aliases equal, messages by role and content, tools by name, tool-use ids ignored.
- Whole JSON body, canonicalised, minus named volatile paths:
  - retake: sorted keys plus dotted ignore paths.
  - cassetter: `json_body` plus `ignore_json_paths`.
  - WireMock recordings: `equalToJson` ignoring array order and extra elements.
  - llm-cassette: normalised request with `id`, timestamps, and request ids removed by key name.
- Whole body after regex scrubbing:
  - Docker cagent: tool call ids, max token fields, reasoning config, tool_choice, and absolute paths rewritten on both sides before comparing.
- Method, URI, and raw or parsed body:
  - LangChain's base VCR config, the LangSmith cache, and evalship's recommended Ruby VCR setting.
- Whole request including headers:
  - rvcr compares method, URI, headers, and body for equality, with a hook to rewrite the request first.
- Embedding similarity:
  - GPTCache and LiteLLM's semantic backends. Built for production cost savings; a near match returning the wrong answer is an accepted risk there and a bug in a test.

Two patterns show up across the strict end of that list:

- Volatile things that tools had to learn to ignore: tool-call ids (cagent, yashshah9, llm-cassette), request ids and timestamps (retake, cassetter, llm-cassette, nock), dated model aliases (yashshah9, and Inspect's alias caveat), max-token and reasoning defaults that depend on outside state (cagent), machine-specific absolute paths in prompts (cagent), transport flags like `stream` (BhagyashriT), per-request timeouts (llm-cassette), and smart quotes or Unicode forms in text (NeMo Guardrails normalises these at comparison time).
- Filtering and matching must agree: VCR.py's `filter_sensitive_data`, cassetter, and retake all run the live request through the same redaction as the stored one, so a scrubbed recording still matches.

## Part 9 - What happens on a miss

- Fail hard (the common CI choice):
  - VCR.py `none`, and `once` when the cassette exists. pytest-recording defaults to `none` and can also block the network.
  - cassetter `none`; rvcr and reqwest-vcr replay (a middleware error); llm-cassette `replay`; baml_vcr `none`; SiluPanda/llm-vcr `replay` (throws).
  - retake `replay`: HTTP 404 with a diff against the closest recording, and exit code 1 for the run. Chosen over 5xx because SDKs retry 5xx.
  - faisalkhan91/cassette: a dial-blocking transport, a hard error, and a counter proving zero dials.
  - Goose TestProvider: a provider error. If the recording file itself is missing on CI, a panic that asks whether you forgot to commit it.
  - Envoy AI Gateway: 409 for an out-of-date interaction, 500 for a missing cassette with no key, 400 for a request with no cassette header that matches nothing.
  - Docker cagent: the proxy answers 500.
  - NeMo Guardrails and LangChain CI: `record-mode=none` and blocked network, so a missing or stale cassette fails the build.
- Call the real model and record:
  - VCR.py `new_episodes`; Polly.js `recordIfMissing` (on by default); nock `record`; retake `auto`; cassetter `new_episodes`; the LangSmith test cache; Envoy AI Gateway when an API key is set and the named cassette is missing; and every cache layer in Part 5.
  - This is the developer-machine mode. Several sources say never to allow it on CI (evalship, Multigrid, LangChain, retake).
- Go to the real network without recording:
  - nock's default `dryrun` mode and mitmproxy's default `server_replay_extra=forward`. Both are easy ways to hit the network from a test by accident.
- Return a fixed status:
  - mitmproxy can answer 204, 400, 404, or 500, or kill the connection. The Rust `wiremock` crate answers 404 to anything unmatched.
- Fall back to a canned reply:
  - I found this only at the edges. WireMock keeps returning the last response of a recorded series once the series runs out. The OpenAI Agents SDK `FakeModel` returns an empty output list when its queue is empty. Every record and replay tool I read treats an unmatched request as an error or as something to record; a quiet canned line on a miss is what our current "pong" backend does, and it is the behaviour these tools were built to avoid.

## Part 10 - When teams pick record and replay over a scripted fake, and when they regret it

### When they pick it

- To get the real wire shape without inventing it. Pydantic AI makes cassettes the default because provider APIs are the judge of correctness. Privorum moved off a mock because its tidy canned answer hid parsing bugs. Multigrid says fidelity is the one thing a cassette is better at: usage layout, chunk ordering, tool calls split across deltas, odd finish reasons.
- To turn a bug report into a test quickly. Pydantic AI's PR that introduced cassettes gives this as a main reason.
- For multi-step agent runs where writing every turn by hand is expensive. retake's comparison rates hand-written mocks as high effort for multi-step agents and a recording as one recording run.
- To run the suite with no API key, including on fork pull requests (retake, Simon Willison's fake-key pattern, NeMo Guardrails' dummy keys).
- To test a gateway or proxy against real provider payloads, including real error payloads (Envoy AI Gateway records bad-request and unknown-model cases; NeMo Guardrails prefers a recordable real 404 over a fabricated one).
- To pin provider contracts and get a reviewable diff when the provider changes something (retake's weekly re-record, Multigrid's scheduled re-record pull request, cassetter's `verify` mode).

### When they keep or go back to a scripted fake

- For logic and control flow: retries, deadlines, breakers, business rules (Multigrid). Pydantic AI kept `FunctionModel` and `TestModel` for dynamic logic and for behaviour that the public API cannot reach.
- For synthetic wire conditions that a real provider will not produce on demand: malformed bodies, retries, cancellation, odd SSE framing. NeMo Guardrails keeps these in a mock-transport suite and allows hand-made "fake cassettes" only as a marked last resort.
- When the harness is in Rust and the team wants full control of ids and stream shape: Codex builds SSE bodies from small constructors on a real mock server and skips recording entirely. Goose proposed a VCR migration for its ACP tests and closed the issue unpursued, after noting how much Rust code and checked-in JSON a VCR approach would add.
- For SDK-level agent loop tests: the OpenAI Agents SDK uses a queue of scripted turns.

### When they regret it

- Silent false greens from loose matching. This is the biggest one, and it has numbers:
  - Pydantic AI: adding the body to the matcher failed 435 tests; in one directory 29 of 30 cassettes were stale; re-recording exposed a real production bug that the stale recordings had been hiding.
  - evalship: 16 of 19 Ruby apps that replay LLM responses would pass with any prompt or model.
- Strict matching that breaks on noise. The same Pydantic AI thread calls body matching "too coarse" and reports failed attempts. Goose's issue says body equality meant rewriting whole request files for minor differences. cagent needs six regex rules to keep replays matching.
- Recordings going stale while still passing. The recording freezes the model at one day. Every write-up says some version of "replay proves your code handles that response, not that the model still gives it".
- Re-recording is costly and gated. It needs keys, money, and a trusted environment. Pydantic AI says contributors routinely under-test because of it. NeMo Guardrails restricts refresh to a trusted environment.
- The recorder is itself buggy code. Privorum found ten defects in its recording path, including a failed re-record wiping a good tape and two scenarios landing on one tape.
- Repository weight and review fatigue. Goose expected "a large amount of JSON blobs" from repeated system prompts. Pydantic AI has thousands of lines of cassettes and tells people not to rely on facts that live only in a cassette because humans skim them. LangChain compresses cassettes with gzip, which then makes them harder to inspect for secrets.
- Streaming fidelity gaps in general tools. VCR.py's httpx adapter joins the whole stream before returning it. rvcr and the cassetter Rust crate buffer bodies. A buffered replay cannot test time-to-first-token, idle timeouts, cancellation mid-stream, or chunk-boundary parsing.
- Transport gaps. HTTP cassette tools cover HTTP only, so Pydantic AI wrote custom cassette formats for gRPC and WebSockets.
- Frozen tools. Where tool traffic is recorded too, tool code never runs on replay (dreaming.press; vcr-langchain and llm-cassette do this on purpose).

## What I could not confirm

- Multigrid's article: direct fetch failed (HTTP 525) three times; content is from search excerpts only.
- `langchain-replay`: named in the dreaming.press article; I did not open the project.
- BAML: I found a third-party `baml_vcr` but did not find a record and replay cache documented in BAML itself.
- Instructor and LlamaIndex using VCR cassettes: GitHub code search returned zero hits; not confirmed either way. Aider fixtures: same.
- Codex using recorded (as opposed to hand-built) SSE fixtures: the test support file I read builds events by hand. I did not survey the whole repo.
- Anthropic SDK "http snapshots": seen in a CONTRIBUTING excerpt; I did not read how matching works.
- httpmock: which request fields a recording matches on by default, and whether it streams SSE on playback.
- stubr: whether it has a record mode.
- reqwest-vcr: whether its matching differs from rvcr's whole-request equality (README only).
- go-vcr: the exact default matcher fields and the list of recorder modes beyond the two that cagent uses.
- Ruby VCR's license and its default matcher from its own docs (the default is stated by the evalship article).
- LiteLLM's default cache key fields.
- The Goose scenario runner on the current main branch (the file path I had returned 404; details are from a search excerpt at an older commit).
- Licenses for Envoy AI Gateway (one SPDX header seen), Mhamadkadiry/llm-vcr and gr-b/baml_vcr (GitHub shows none detected), NVIDIA NeMo Guardrails and LiteLLM (GitHub shows no single SPDX id).
- Binary bodies: I confirmed how tools store them (base64 in rvcr and retake, compressed-unless-decoded in VCR.py). A write-up from a team bitten by it is still missing; the only warning I saw is the VCR.py docs' own note about secrets inside compressed bodies.
- Cost: the sources talk about cost only in general terms (tokens per live run, a budgeted key for re-recording). Measured dollar figures for recording runs remain unconfirmed.

## Lessons and pitfalls

Each point lists who reported it.

### Matching

- The matcher is the design (Multigrid). Too loose and a test replays someone else's answer and passes for the wrong reason. Too strict and every edit or random id is a miss.
- URL-plus-method matching is the default in VCR.py, Ruby VCR, and cassetter, and it is wrong for LLM endpoints because every call is a POST to one URL (evalship, Pydantic AI issue 6993).
- If you match on the body, canonicalise it first: parse the JSON, sort keys, and drop named volatile paths (retake, cassetter, WireMock, Multigrid).
- Tool-call ids are the classic miss. The provider invents a new id in each response, the harness echoes it in the next request, and a byte comparison fails. Normalise them on both sides (cagent's `call_ID` regex, yashshah9's semantic matcher), or generate them deterministically in a scripted fake so the problem never appears (Codex passes its own `call_id` into its event constructors).
- Beware normalising by key name alone: llm-cassette drops every request field named `id`, including a tool call's `id`.
- Other fields that drifted in real suites: max-token and reasoning defaults, `tool_choice` added by a later client version, absolute paths inside prompts (all cagent), dated model aliases (yashshah9, Inspect), smart quotes and Unicode forms (NeMo Guardrails).
- Do not put timestamps, UUIDs, or random samples into prompts in the first place. Make the clock and the random source injectable and fix them in tests (Multigrid). VCR.py's cassette metadata can store a seed so record and replay agree.
- Leave headers out of the key (nock, retake, Multigrid), or strip them before matching (rvcr needs this done by hand).
- A scenario name plus call order is a legitimate key (Envoy, retake sessions, Privorum), as long as something else checks the request content. Pydantic AI's answer is to assert on the request the live code built, captured by a hook that also fires on replay, and not on the request stored in the cassette.
- When a match fails, print a diff against the closest recording (retake, rvcr `rich_diff`, llm-cassette, go-vcr debug mode, Goose's proposed 417 with a JSON diff). "No cassette found" on a large JSON body is close to useless.

### Streaming

- Record SSE as a list of events, not as concatenated text (retake, yashshah9, Multigrid, faisalkhan91/cassette, NeMo Guardrails).
- Keep timing optional and off by default. Stored timings make cassette diffs noisy (retake). Use fixed small delays (cagent's 15 ms) or recorded offsets only for tests about timeouts and stalls.
- General HTTP cassette tools buffer the whole body (VCR.py httpx adapter, rvcr, cassetter Rust crate). That is fine for checking that the assembled message is right and useless for testing incremental delivery.
- Be clear what a replayed stream proves. NeMo Guardrails states it as assembled SSE content, not socket timing, and tests delivery order, interruption, and cancellation with separate gated fakes.
- Decide whether "same" means the same bytes or the same assembled message (faisalkhan91/cassette's WIRE versus SEMANTIC equivalence).
- Record the ugly cases deliberately: a multi-byte character split across chunks, tool arguments spread over many deltas, a stream with no terminal event (Multigrid, faisalkhan91/cassette).
- Use captured SSE exactly as captured; do not hand-edit it (Goose issue 6969, Pydantic AI's rule against editing cassettes).

### Tool-call rounds

- Each round is one more request whose body contains everything before it, so one changed tool output invalidates every later match in that test. Matching by conversation hash (Goose TestProvider) makes rounds order-independent but ties the recording to exact tool output.
- A wire-level recording of the model leaves locally executed tools running for real. A recording that also captures tool traffic freezes the tools (dreaming.press). Know which one you have.
- Assert that every recorded interaction was consumed, so a test cannot pass by skipping a round (faisalkhan91/cassette, NeMo Guardrails, VCR.py `all_played`). llm-cassette's pytest plugin leaves this check off by default, which it lists as a limit.

### Misses and CI rules

- In CI: replay only, fail on a miss, block the network (pytest-recording's default `none` and `block-network`, LangChain's `record-mode=none`, NeMo Guardrails, retake `replay`, Goose's CI panic). Never record on CI (evalship, Multigrid).
- Avoid defaults that quietly reach the network: nock `dryrun`, mitmproxy `forward`, Polly.js `recordIfMissing`, LangSmith's `new_episodes` cache.
- Answer a miss with a 4xx, not a 5xx, because LLM SDKs retry 5xx and turn a clear failure into a slow one (retake). cagent's 500 and Envoy's 500 are exposed to this.
- Also fail the run if any call went unmatched, even when the tests passed, in case the code under test swallowed the error (retake).
- Skip recording 429 and 5xx responses unless you want them, so SDK retries do not end up as extra interactions (retake).

### Secrets

- Scrub at write time and by default: auth headers always (retake, cassetter, cagent, LangChain base config). VCR.py and rvcr leave this to you.
- Header filtering is not enough. Tokens also appear in bodies and URLs. LangChain's script scans finished cassettes for token shapes and aborts; retake has a `check` command for CI and pre-commit; Privorum fails the pipeline on a leftover secret.
- Pattern-based redaction cannot see personal data in prompts. Record with synthetic data and review cassettes like code (retake).
- Redact at save time, not capture time, when the running test still needs the real value (go-vcr's hook advice).
- Compressed bodies hide secrets from text filters. Decode before storing (VCR.py `decode_compressed_response`, used by LangChain and Pydantic AI).
- Use a fake key on replay and make the filter and the code agree on it (Simon Willison, VCR.py docs, Pydantic AI's `mock-api-key`).

### Drift and re-recording

- Cassettes rot in two directions: your code changes what it sends (Pydantic AI, evalship), and the provider changes what it returns (Multigrid, retake).
- Make re-recording a deliberate, reviewed act: a `rewrite` mode, then a playback run, then a diff review (Pydantic AI, NeMo Guardrails make targets, Multigrid's re-record-all on a branch).
- Schedule a live re-record with a separate budgeted key and read the diff (retake weekly job, Multigrid). Polly.js and Ruby VCR have expiry settings; Multigrid suggests stamping each cassette with date and model id and warning past an age threshold.
- One cassette per test, recorded by that test. Do not copy cassettes between tests or edit them by hand (Pydantic AI). Reuse across tests is possible (Goose reuses request and response pairs) but must be a stated choice.
- Protect good recordings from bad runs: do not save on test failure (VCR.py `record_on_exception=False`), and do not let an empty recording overwrite a full one (Privorum's bug).
- Prune recordings the test no longer makes (VCR.py `drop_unused_requests`, Polly.js `keepUnusedRequests: false`). retake warns when recorded interactions were never replayed.
- Test the recorder itself (Privorum).
- A key-definition change invalidates old entries. Inspect AI tells users to clear caches after it added constructor arguments to its key; go-vcr says a major version bump may need new cassettes.

### Size and readability

- Repeated system prompts make every interaction large (Goose). Options seen: gzip the cassette (LangChain), bzip2 (rvcr), store bodies as parsed JSON for readable diffs (NeMo Guardrails, retake), truncate audio and base64 (Pydantic AI), move big bodies to side files (WireMock).
- Humans skim cassettes. Put the facts a test depends on into assertions or snapshots in the test file (Pydantic AI). Pair cassettes with snapshot tests of the output (Simon Willison with syrupy, Pydantic AI and NeMo Guardrails with inline-snapshot).

### What record and replay does and does not prove

- It proves: the code builds a request, parses a real response shape, assembles a real stream, and handles real error payloads.
- It does not prove: that today's model still answers that way, or that a prompt change still produces a good answer. That needs live evals on a schedule (evalship, retake, Privorum, ajayvnkt/agent-vcr's own README).
- A recording is the only reproducible artefact of a model interaction, because the same request returns different bytes tomorrow (Multigrid). That is the argument for committing and reviewing it.
