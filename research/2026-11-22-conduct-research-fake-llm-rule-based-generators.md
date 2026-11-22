---
produced: 2026-11-22
title: Fake LLM for tests - non-neural text generators (ELIZA, AIML, RiveScript, ChatScript, Markov chain, lorem ipsum, Tracery, JSON Schema fakers, deterministic echo and directive mocks), PyBot reception and Rust ports, tiny CPU models (stories260K, SmolLM2-135M), fit behind an OpenAI chat-completions endpoint with SSE streaming and tool calls
---

# Fake LLM research: rule-based and other non-neural generators as a model stand-in

## What this file is

We want a crude fake model for automated tests of a Rust agent harness and gateway that speak the OpenAI chat-completions API (JSON and SSE streaming, with tool calls). It should cost microseconds to a few milliseconds per reply, run locally, be deterministic, and still react to what was said and to earlier turns. This file collects real projects for each candidate approach and says how each one would fit.

- Research date: 2026-11-22
- Method: web searches, then opening the real pages (project READMEs through raw GitHub URLs, crates.io pages, docs sites, Hugging Face pages). For a handful of small repos I downloaded single source files to count lines and read the logic.
- Label "(opened)": I fetched that page or file myself and read it.
- Label "(search capture)": I only saw the copy of the page text that the search tool returned. Treat those facts as one step weaker.
- Star counts and download counts are left out on purpose, they move too fast to matter here.

## 1. ELIZA-style pattern and template bots

The short version: ELIZA is the smallest thing that reacts to its input and remembers a little, and people already serve it through OpenAI-compatible endpoints exactly for the reason we care about.

### eliza-rs, the `eliza` crate (Rust)

- URL: https://github.com/arosspope/eliza-rs and https://crates.io/crates/eliza (both opened)
- Version and age: 2.0.1, last published 2023-12-26
- License: MIT or Apache-2.0
- Dependencies: `regex`, `rand` 0.7, `serde`, `serde_json`, `log`, `env_logger`
- Size (my count from the raw files on master): `src/lib.rs` 763 lines, `src/script.rs` 227, `src/alphabet.rs` 129, `src/main.rs` 69, so about 1,190 lines of Rust. The bundled DOCTOR script `scripts/doctor.json` is 878 lines (25.6 KB). The example `scripts/pirate.json` is 24 lines (446 bytes), which shows how small a working script can be.
- Script format (JSON): `greetings`, `farewells`, `fallbacks`, `transforms`, `synonyms`, `reflections`, and `keywords`. Each keyword has a `key`, a `rank`, and `rules`. Each rule has `memorise` (bool), a `decomposition_rule` (a Rust regex), and `reassembly_rules` (templates that use `$1`, `$2` for capture groups). Docs: https://docs.rs/eliza/latest/eliza/script/index.html (search capture)
- How a reply depends on input: lowercase and transform the input, split into phrases, rank the keywords found, try each keyword's regex rules, fill the chosen template with captured text after swapping pronouns through `reflections`. A template can also be a GOTO to another keyword.
- How a reply depends on history: two pieces of state live in the `Eliza` struct. Rules marked `memorise` push their finished reply onto a memory queue, and that queue is popped later when nothing matches. A usage counter per template makes the engine pick the least-used template next time, so repeating the same input walks through the alternatives in order.
- Determinism: template choice is the least-used rule, which is deterministic. Greeting, farewell and fallback lines are picked with `rand::thread_rng()`, which is unseeded. The README says ELIZA is "somewhat deterministic" and shows `assert_eq!("bar", e.respond("foo"))` unit tests on scripts. For a fully deterministic fake you would swap the three random picks for a seeded or rotating choice.
- API: `Eliza::from_file(path)`, `Eliza::from_str(json)`, `respond(&mut self, input) -> String`. The script can be compiled into the binary as a string.
- Speed: none stated. It is regex matching over a short script, so I would expect it well inside our budget, but that is my expectation, not a measured number.
- Fit behind chat-completions: good for the text part. The server is stateless per request in our world, so the fake would build a fresh `Eliza` and replay the user messages from the request, then return the last reply. Streaming is just splitting the reply into word chunks. Tool calls are outside ELIZA's vocabulary, so that part has to come from another layer (see sections 4 and 5).

### elizaOS "classic ELIZA" model plugin (TypeScript and a Rust crate)

This is the clearest case of ELIZA being wired in as a model provider so an agent runs with zero inference.

- Rust crate: https://crates.io/crates/elizaos-plugin-eliza-classic (opened). Version 2.0.0, published 2026-03-17, MIT. Description: "Classic ELIZA pattern matching plugin for elizaOS - no LLM required". API shown in its README: `ElizaClassicPlugin::new()`, `generate_response(text)`, `get_greeting()`. Its tests also call `reset_history()`. Dependencies: `regex`, `rand` 0.8, `serde`, `serde_json`, `lazy_static`, `anyhow`, `thiserror`, with optional `wasm`, `ffi` and `ipc` features.
- Its tests (search capture of the docs.rs source view) include a "golden transcript deterministic" test where a fixed list of inputs must give a fixed list of replies, a test that repeating "computer" three times gives three different replies in order, and a memory-recall test ("my car is broken" comes back later as one of four "Earlier you said..." style lines).
- TypeScript plugin `@elizaos/plugin-eliza-classic` (search capture of the README and AGENTS.md in the elizaOS monorepo; my own fetch of that GitHub path returned 404 today, so the current location is unconfirmed): it registers model handlers for every text model type and answers with about 16 keyword regexes, falling back to "Please go on.". It also returns a deterministic 1536-dimension hashed bag-of-words embedding. The README lists the use cases as offline zero-cost agent testing and CI pipelines that need responses without network calls. It reads the last user turn out of the prompt with a `User:|Human:|You:` regex.
- Related (search capture): the elizaOS A2A server example says that when `OPENAI_API_KEY` is unset it runs in a deterministic "ELIZA classic" mode.
- Fit: the Rust crate is a ready-made dependency, but it is tied to the elizaOS project, has very few users, and I did not read its source. I would treat it as proof of the idea more than as a dependency.

### elizacp, ELIZA as a test agent (Rust)

- URL: https://crates.io/crates/elizacp (opened), repository https://github.com/symposium-dev/symposium-acp
- Version: 12.0.0 (2026-03-16), MIT or Apache-2.0
- What it is: "Classic Eliza chatbot as an ACP agent for testing". It speaks the Agent-Client Protocol over stdio JSON-RPC, which is a different protocol from OpenAI chat-completions.
- State: a `HashMap<SessionId, Eliza>`, one ELIZA per session.
- Stated purpose: "Lightweight agent with deterministic pattern-based responses" for testing clients and protocol work "without heavy AI infrastructure".
- Worth copying: its README lists "Tool use triggers (e.g., "what's the weather" -> tool call)" as a planned extension of the pattern table. That is the same idea we want: a pattern that fires a tool call.

### Eliza-GPT, ELIZA behind chat-completions (Python)

- URL: https://github.com/miguelgrinberg/eliza-gpt (README opened, `src/eliza_gpt/v1.py` opened)
- License: MIT (from `pyproject.toml`)
- Why it exists, in the author's words: "it is great to have a fast and free service with negligible resource consumption that can be used as a stand-in for a real LLM when the quality of the responses do not matter, such as while doing development or testing."
- Size: the whole OpenAI layer `v1.py` is 99 lines. The ELIZA engine is the vendored `eliza-py` by Riccardo Di Maio (https://github.com/rdimaio/eliza-py, MIT, opened): about 17 KB of Python across five files plus `doctor.json` (28 KB) and `general.json` (2.5 KB).
- How history is used: the handler keeps only messages with role `user`, then runs all of them through ELIZA in order with a fresh memory stack and returns the last reply. So the server is stateless and rebuilds ELIZA's memory from the request every time.
- Determinism: it honours the request's `seed` field by calling `random.seed(seed)`.
- Streaming: yes, SSE. It splits the reply on spaces and sends one word per chunk, then `[DONE]`.
- Supported request fields: `messages`, `stream`, `seed`. It reads only user messages and skips `tools`, `max_tokens`, `stop`, `n`, temperature and logprobs. Only `/v1/chat/completions` exists, other paths return 404.
- Speed: simulated response time is a setting, and "Set to 0 to have near immediate responses".
- Fit: this is the design to copy for the text branch. A Rust version is small.

### chabeau-server, ELIZA plus test directives (Node.js)

- URL: https://github.com/permacommons/chabeau-server (README opened, source file sizes checked)
- License: CC0-1.0 (public domain)
- Tagline: "A lightweight Node.js server providing an ELIZA chatbot through an OpenAI-compatible API. Perfect for testing chatbot applications without requiring an actual LLM."
- Size: `eliza-bot.js` 566 lines, `server.js` 280 lines, one dependency (`smol-toml`).
- Endpoints: `POST /v1/chat/completions` with `stream`, `GET /v1/models`, `GET /health`. The token "badtoken" triggers an auth failure on purpose.
- Directive triggers in the user message: `!`, `!!`, `!!!` return one to three long paragraphs; `@` returns code blocks; `|` returns markdown tables; `%` returns links; `~` returns multi-script text; `&` returns hard emoji; `>` and `<` change streaming speed (baseline 50 tokens per second); `)` and `(` change latency; the word `system` returns the system prompt it received and `raw` returns the full request payload as JSON.
- Determinism: reply choice uses `Math.random()` (seen in the source), so it is unseeded.
- Fit: the ELIZA part is thin (a small pattern list). The useful idea is the directive characters and the `system` and `raw` echo commands for debugging what the gateway actually sent.

### ElizAPI (TypeScript on Cloudflare Workers)

- URL: https://github.com/computerphilosopher/ElizAPI (README opened)
- License: MIT
- Engine: the `elizabot` npm package, a Node port of Norbert Landsteiner's elizabot.js (https://www.masswerk.at/elizabot/)
- History: "Since the server is stateless, it replays previous messages to rebuild ELIZA's internal state", so the client must send the whole `messages` array each time.
- Streaming: the README shows only plain JSON examples. Streaming support is unconfirmed.

### Other small ones

- gmerritt/Eliza_GPT_lambda (https://github.com/gmerritt/Eliza_GPT_lambda, search capture): an AWS Lambda wrapper around Eliza-GPT. Uses the last user message, returns SSE as one body with a final `data: [DONE]`.
- wadetb/eliza (https://github.com/wadetb/eliza, opened): MIT. `eliza.py` is 236 lines and the `doctor.txt` script is 360 lines. A good yardstick for "how small is a useful one": roughly 250 lines of engine plus a 12 KB script gives the full classic DOCTOR behaviour including memory.

### Takeaways for ELIZA

- A useful engine is 250 to 1,200 lines plus a 12 to 28 KB script. A purpose-written script for tests could be much smaller (the pirate example is 24 lines).
- Every OpenAI-compatible ELIZA server I found handles history the same way: stay stateless, replay the user turns from the request.
- All of them answer with text only. None of the ELIZA servers I opened emits tool calls.

## 2. AIML / ALICE and other rule engines (Program-Y, ChatScript, RiveScript, Rasa rules)

The short version: these are bigger engines aimed at authoring large hand-written bots. The one that fits a Rust test binary is RiveScript, which got a native Rust port in 2026. The classic AIML data is reusable only under GPL or LGPL terms.

### AIML engines and the ALICE data

- Program-Y (https://github.com/keiffster/program-y, search capture): Python 3, "fully compliant AIML 2.1 chatbot framework", described as "an entire platform". GitHub shows its license as "Other". It is a platform, far heavier than we need, and it is Python.
- `aiml` / PyAIML fork (https://github.com/ArtificialIntelligenceToolkit/aiml, search capture): a pure-Python AIML 1.0.1 interpreter, `pip install aiml`. Engine is 2-clause BSD. It bundles the "standard" AIML set and the Free ALICE AIML set v1.9.
- Free ALICE AIML set (https://code.google.com/archive/p/aiml-en-us-foundation-alice/, search capture): license GNU Lesser GPL. Includes about 54,000 yes/no style questions and statements from the GAC-80K collection translated to AIML. Mostly AIML 1.0.1 compatible with some Pandorabots extensions, and the files must load in a set order.
- hosford42/AIML_Sets (https://github.com/hosford42/AIML_Sets, search capture): repackages ALICE, Square Bear's Mitsuku files and the standard set, and states GPL v3 for everything.
- Rust: `aiml_ported` (https://crates.io/crates/aiml_ported, opened). Version 0.1.5 from 2021. Its own description says: "Port of python-aiml to rust. WARNING: This runs much slower than python-aiml. I recommend you use the saved brain." License field is "non-standard". I found no other Rust AIML interpreter on crates.io with a search for "aiml".
- Is the data reusable: legally yes under LGPL or GPL, which is a poor match for embedding in a permissively licensed test crate. Practically it is tens of thousands of open-domain chit-chat categories, which is the opposite of the small, predictable script a test fake wants.
- How replies depend on input and history: AIML matches the input against patterns with wildcards, and its `that` and `topic` mechanisms condition on the bot's previous reply and the current topic. I am stating that from general knowledge of AIML, I did not open a page for it in this session.

### ChatScript (C++)

- URL: https://github.com/ChatScript/ChatScript (the older https://github.com/bwilcox-1234/ChatScript is the unmaintained original location)
- License: MIT (search capture of `license.txt`, copyright Bruce Wilcox 2011-2022)
- Speed, from its own overview page (opened): "A low-grade server running our most advanced chatbot can generate a response in a few milliseconds", and the largest bot "runs on an iPhone in 18Mb of memory".
- Rule counts it quotes: "chatbots of 12K to 16K ChatScript rules", and a virtual-patient project that went from "hundreds of thousands" of AIML rules to under 2,000 ChatScript rules.
- History: it tracks each user across turns (conversation position and learned facts).
- Embedding (search capture of the External Communications wiki page): add the `.cpp` files to your build, define `NOMAIN`, then call `InitSystem(...)`, `PerformChat(user, usee, incoming, ip, output)` and `CloseSystem()`. It ships its own dictionary and ontology.
- Fit: it meets the speed budget by its own claim, but it means a C++ engine with a dictionary behind FFI in a Rust test binary. Too much machinery for a fake.

### RiveScript (native Rust port since 2026)

- URLs: https://github.com/aichaos/rivescript-rs (README opened), https://crates.io/crates/rivescript (search capture), author's post https://www.kirsle.net/rivescript-in-rust (search capture, dated 2026-05-01)
- Version: 0.3.0, published 2026-04-30. License: MIT.
- Status in the README: "Current Status: Beta". It is "feature complete", implements all commands and tags, and passes the RiveScript Test Suite.
- Script format: plain text, for example `+ hello bot` on one line and `- Hello, human!` on the next. Triggers use a simplified regex with wildcards. Sources load from a directory, a file, or a string through `bot.stream("...")`, followed by `bot.sort_triggers()`.
- API: `bot.reply("local-user", &message).await` returns the reply. It is async and depends on `tokio`.
- History: user variables are stored per username, in memory by default, with a Redis option. The feature checklist also covers topics, redirects, arrays and weighted random replies.
- Determinism: replies with several alternatives are chosen at random (the feature list says "(weighted) random responses" and `{random}`). I did not find a seed setting in the part of the README I read, so seeding is unconfirmed. A script with one reply per trigger is deterministic by construction.
- The project's own framing (search capture of rivescript.com): put RiveScript "in front" so it answers the cases you scripted, and fall back to an LLM for the rest.
- Fit: workable if we want a real script language with variables and topics without writing an engine. Costs: an async API, a `tokio` dependency, a beta crate, and per-user state held inside the engine, where our fake wants the reply to be a pure function of the request. To stay stateless we would create a bot per request and replay turns, same as ELIZA.

### Rasa rules

- URL: https://rasa.com/docs/rasa/rules/ (opened)
- What a Rasa rule is: "a type of training data used to train your assistant's dialogue management model". Rules map an intent to an action for short fixed paths, with optional conditions on slots, and need `RulePolicy` in the model configuration.
- Fit: poor. Rules sit on top of Rasa's intent classification and training pipeline, so they are part of a Python ML framework. The idea worth keeping is the shape: intent, optional condition on stored slots, then action.

### Takeaways for rule engines

- Light and embeddable in Rust today: `rivescript` (beta, async). ELIZA-style engines from section 1 are lighter still.
- AIML data: reusable only under LGPL or GPL, and it is the wrong kind of data for a predictable test fake.
- ChatScript: fast by its own numbers, but a C++ FFI dependency.

## 3. Markov chain and n-gram generators

The short version: Markov text is good filler for streaming and length tests, and it can be made deterministic with a seeded RNG. On its own it does not answer the user, so it works as a fallback layer under rules. One project already ships exactly that combination.

### Rust crates

- `lipsum` (https://github.com/mgeisler/lipsum, README opened; https://docs.rs/lipsum/latest/lipsum/, opened). Version 0.9.1, MIT. An order-two Markov chain (next word depends on the previous two) trained on the first book of Cicero's De finibus. Functions with a caller-supplied RNG: `lipsum_with_rng`, `lipsum_words_with_rng`, `lipsum_title_with_rng`. The same chain works on other text: create a `MarkovChain` and call `learn`. Since 0.9.0 it has no dependency on system randomness. The docs.rs source view (search capture) shows the default generator is `ChaCha20Rng::seed_from_u64(97)` and an iterator of words (`Words`), which maps straight onto streaming.
- `markov` (https://crates.io/crates/markov, opened; https://github.com/aatxe/markov). Version 1.1.0 from 2020, license CC0-1.0, in "passive maintenance mode". Generic `Chain<T>`, with `feed_str`, `generate_str`, iterators, and higher orders through `Chain::of_order(2)`. It depends on `rand` 0.7. Whether it accepts a seeded RNG is unconfirmed, the pages I read show only the unseeded calls.
- `markov_strings` (https://docs.rs/markov_strings/latest/markov_strings/struct.Markov.html, search capture). Version 0.1.6. Its docs give the clearest quality-versus-table guidance: a state size of 1 "will mostly output non-sense gibberish", 2 "can produce interesting results, when correctly filtered", 3 or more is "more intelligible" but needs source material big enough to stay random. Building the corpus is slow ("adding 50.000 tweets ... takes more than 20 seconds"), so it has `export()` and `from_export()`.
- `markovify-rs` (https://crates.io/crates/markovify-rs, search capture). Version 0.1.2 from 2026-03, MIT. A port of Python's markovify: default state size 2, JSON save and load, `compile()`, sentence generation with a character limit. Seeding is unconfirmed.
- Python `markovify` for reference (https://github.com/jsvine/markovify, README opened): default state size 2, `compile()` "for improved text generation speed and reduced size", `to_json()` export, and it keeps the original corpus by default so it can reject sentences copied verbatim.

### Quality versus table size, in plain terms

- Order 1 needs almost no data and gives word salad. Order 2 is the usual default and reads like plausible nonsense. Order 3 reads better but mostly replays the corpus unless the corpus is large.
- Table size follows the corpus: one entry per distinct word pair (order 2) with its list of followers. A few kilobytes of sample text gives a table of a few thousand entries.
- None of the pages I read state a generation speed. It is a hash lookup per word.

### Markov or lorem ipsum already used as a fake LLM

- shishberg/llmock (https://github.com/shishberg/llmock, README opened). Go. "A mock LLM API server for testing" with OpenAI `/v1/chat/completions` and Anthropic `/v1/messages`, SSE streaming, and a "Markov chain fallback - generates plausible-looking LLM text when no rule matches". Rules are regexes on the last user message with capture groups (`$1`), `${input}` for the whole message, and `{{markov:N}}` for about N words of Markov text. Settings: `seed` ("RNG seed for deterministic output"), `corpus_file` for your own training text, `token_delay_ms`. A rule can attach a `tool_call` with arguments built from capture groups. With `auto_tool_calls` on, when the request has tools and no rule fires, it "picks a random tool and generates arguments from its JSON schema", respecting types, enums, formats and required fields. It also handles tool-result message sequences, fault injection, and can be used as a Go library with `llmock.New(llmock.WithRules(...), llmock.WithSeed(42))`. License: the README says "See LICENSE", I did not open that file. This is the closest existing match to "rules plus Markov filler plus schema-made tool calls".
- simonw/llm-markov (https://github.com/simonw/llm-markov, README and source opened). A plugin that adds a model called `markov` to the LLM command-line tool. The source is 67 lines. It builds the transition table from the words of the prompt itself on every call, then emits 100 words by default, one token at a time, with a simulated 0.02 second delay per token when streaming. It uses unseeded `random.choice`. License Apache 2.0. The neat idea here: train the chain on the prompt, so the reply is made of the user's own words and needs zero stored data.
- enrico9034/llmock (https://github.com/enrico9034/llmock, README opened). Go, MIT. "A fake LLM server for load-testing everything that sits in front of an LLM" that "answers with lorem ipsum at exactly the length, speed and failure rate you ask for" and "does no inference". Control travels in the prompt: `[[mock tokens=500 tps=20]]` in the system prompt or last user message, "so it works through proxies that drop custom headers". Keys include `tokens`, `delay`, `tps`, `chunk`, `error`, `fail_after`, `finish_reason` and `seed` ("deterministic text and ranges"). Its measured numbers, from a container limited to 4 CPUs on a laptop: 55,794 requests per second non-streaming with p50 latency 3.4 ms, 99.6% of first tokens under 1 ms after the handler started, native binary answering `/health` 7 ms after launch, 11.4 MB image. Tool calls are not mentioned in the README.
- llm-d-inference-sim (https://github.com/llm-d/llm-d-inference-sim, README opened). Go. A vLLM simulator with two content modes: echo (returns the last user message) and random (the default: concatenates sentences from a built-in list until the target length, or looks up a stored reply by a hash of the prompt in an optional SQLite dataset). It models time to first token and inter-token latency.
- Rustybin's `random` mode (section 5) builds sentences from a fixed vocabulary "seeded by a hash of the model and prompt: identical requests get identical text".

### Fit behind chat-completions

- Streaming: natural. A Markov or lipsum word iterator yields one SSE delta per word.
- Determinism: seed the RNG from a hash of the conversation and the output is a pure function of the request. `lipsum` and `tracery` take any `rand::Rng`; for `markov` this is unconfirmed.
- Input dependence: weak unless the chain is trained on the prompt (the llm-markov trick) or the seed comes from the prompt hash. It never answers a question.
- History dependence: only through the seed, or by training the chain on all earlier messages.

## 4. Grammar and template generators, and JSON Schema fakers

The short version: this is the piece that makes tool calls and structured output work without a model. Several Rust crates generate a JSON value that validates against a schema from a seeded RNG, and several mock servers do the same thing in a few dozen lines with fixed rules (default, then first enum value, then a placeholder per type).

### Tracery (grammar expansion)

- Original: https://github.com/galaxykate/tracery (readme opened). JavaScript, `tracery.js` is 32 KB. The license file is Apache 2.0 (opened). A grammar is a JSON object mapping symbols to lists of rules. Rules reference symbols as `#symbol#`, with modifiers such as `.capitalize`, `.a`, `.s`, and can save a choice for reuse, as in `#[hero:#name#][heroPet:#animal#]story#`.
- Rust port: `tracery` crate (https://crates.io/crates/tracery and https://docs.rs/tracery/latest/tracery/, search capture). Version 0.2.1 from 2021, MIT or Apache-2.0. Grammars come from a `grammar!` macro, a map, or the original JSON format. `execute(&key, &mut rng)` and `flatten(&mut rng)` take any `rand::Rng`, so a seeded RNG gives repeatable text. Dependencies include `pest` and `rand` 0.8.
- How it reacts to input: by itself it ignores input. To react, the fake would push captured words from the user message into the grammar as symbols (for example a `topic` symbol) before expanding. I did not find a project doing that for an LLM mock.
- Fit: a nice way to get varied, well-formed sentences from a tiny data file. Optional.

### Lorem ipsum and faker crates

- `lipsum`: see section 3. Seedable, MIT, iterator of words.
- `fake` crate (https://github.com/cksac/fake-rs, README opened). Version 5 in the README, MIT or Apache-2.0. Generates names, addresses, internet values, lorem `Words`, `Sentence`, `Paragraph`, and markdown pieces (`BulletPoints`, `Code`, `Link`, block quotes). It has `#[derive(Dummy)]` for whole structs and `Faker.fake_with_rng(r)` with `StdRng::from_seed(seed)` for fixed output. Optional `serde_json` feature.
- Use for us: filler values for string fields by field name (email, city, date), and markdown-shaped replies for rendering tests.

### JSON Schema fakers

- `noson` (https://crates.io/crates/noson, opened; repo https://github.com/glennib/noson). Rust, version 0.2.2 from 2026-08, MIT or Apache-2.0. `noson::generate(&schema, &mut rng)` takes a `serde_json::Value` schema and returns "a random JSON value that validates against it". "The caller provides the random number generator, so output is reproducible when a seeded RNG is used." Required properties are always included, optional ones with 50% probability, recursion up to depth 16. Covers types, unions, numeric and length bounds, `multipleOf`, tuples, `uniqueItems`, `contains`, `pattern` (string built from the regex), formats (date-time, date, time, duration, uuid, email, uri, hostname, ipv4, ipv6), `enum`, `const`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`, and local `$ref`. External `$ref` returns an error. Dependencies: `rand` 0.10, `regex-syntax`, `serde_json`, `jiff`, `uuid`. This is the best direct fit I found for "schema-valid tool arguments from a seed" in Rust.
- `vld-fake` (https://crates.io/crates/vld-fake, search capture). Generates values "that pass vld schemas (or any JSON Schema)" with optional seeds. The crates.io search API today shows its description as "DEPRECATED: moved to vld 0.8 (vld::fake)", so the function now lives inside the `vld` crate.
- `json-schema-faker` Rust crate (https://crates.io/crates/json-schema-faker, search capture). Version 0.1.0. It is a Wasmtime wrapper around the JavaScript library and needs Javy and npm at build time. Its README says: "There was no way to generate test data in Rust that complied with a provided JSON Schema, and no time to build one." Too heavy for a test fake.
- `datagen-rs` and `jgd-rs` (search capture): both generate JSON from their own schema dialects, which differ from standard JSON Schema. `jgd-rs` supports a `seed`. They do not consume a tool's `parameters` schema directly.
- JavaScript `json-schema-faker` (https://github.com/json-schema-faker/json-schema-faker, README opened). MIT. Rewritten in TypeScript with zero production dependencies. "Deterministic output via seeded PRNG (Mulberry32)". Supports JSON Schema 2019-09 and 2020-12, composition keywords, `$ref` with cycle detection, formats, regex patterns, and a `useDefaultValue` / `useExamplesValue` option. Ships a `jsf` command and standalone binaries. Useful as a reference for which keywords matter.
- `schemars` examples and `jsonschema`-based fakers: I did not find or open anything that generates fake instances with `schemars`. `noson` uses the `jsonschema` crate only in its own tests, to check its output validates.

### How existing mocks fill tool arguments (the simple recipes)

- axium-lab/llm-mock (https://llm-mock.dev/api-openai.html, search capture): "Arguments are synthesized from the tool's JSON Schema - `default` first, then the first `enum` value, then a placeholder per type - covering the `required` properties, or all of them when the schema declares none."
- Rustybin (https://docs.rustybin.dev/ai/behaviour.html, opened): arguments come from the schema (required and optional properties, first enum value, nested objects and arrays, `$ref` into `$defs`, `anyOf`/`oneOf`/`allOf`, string formats and bounds, number bounds, `const`, `default`). "String values are chosen from the property name (`email`, `city`, `date`, ...) and the user text, so `location` becomes "Paris" for "What is the weather in Paris?"." Generation is bounded: at most 2,000 values, 12 levels deep, strings up to 1,024 characters.
- Pydantic AI `TestModel` (https://ai.pydantic.dev/testing/, opened): "There's no ML or AI in `TestModel`, it's just plain old procedural Python code that tries to generate data that satisfies the JSON schema of a tool." Its example output is `{'location': 'a', 'forecast_date': '2024-01-01'}`.
- shishberg/llmock: "generates schema-compliant arguments, respecting types, enums, formats, and required fields".

### Fit behind chat-completions

- A tool call is just `finish_reason: "tool_calls"` plus a JSON string of arguments. A schema faker gives that string. Streaming it means slicing the JSON string into `delta.tool_calls[].function.arguments` pieces.
- `response_format` with `json_schema` is the same generator pointed at the response schema.
- Determinism: seed from a hash of the conversation, or skip randomness entirely with the fixed recipe (default, first enum, placeholder).

## 5. Deterministic pseudo-model designs people use for tests

The short version: the field has converged on a few designs. Scripted lists and fixtures (what we have now), echo, header or prompt directives, and a newer kind that derives the reply from the request itself: call a tool when one is forced or mentioned, quote the tool result on the next turn, emit schema-valid JSON when asked, otherwise canned or hash-seeded text. That last kind is the closest to what we want, and one of the examples is written in Rust.

### Reply as a pure function of the request

- Rustybin mock LLM (docs https://docs.rustybin.dev/ai/behaviour.html and https://docs.rustybin.dev/ both opened; repo https://github.com/coldbathcode/rustybin, search capture). Rust, single binary, license AGPL-3.0 with a commercial license on offer. Speaks seven providers' wire formats including OpenAI chat-completions. Its docs say: "Nothing is generated by a real model: replies come from keyword tables, scripted rules, the prompt itself or a seeded generator, so the same request gives the same answer". The reply is chosen in this order:
  1. Echo mode: the rendered prompt exactly as received, one line per part (`system: ...`, `tools: ...`, `user: ...`, `assistant: [tool_call name] {...}`, `tool (name): ...`).
  2. The last messages hold tool results: a final answer quoting them.
  3. Tools are supplied and tool choice is not `none`: a tool call when a tool is forced or required, or when the last user message mentions a tool by name, by a significant part of its name, or by a significant word of its description. The best scoring tool wins. One tool call per reply.
  4. Structured output requested: JSON valid against the schema.
  5. Otherwise text from the mode: `canned` (a keyword table on the last user message: greetings, code, JSON, long text, else a default paragraph), `scripted` (trigger words such as `lorem N`, `refuse`, `json`, `markdown`, `echo`, plus fake PII or secrets for guardrail demos), or `random` (hash-seeded sentences).
  - Mode comes from an `X-Rustybin-Mode` header or from a segment of the model name such as `gpt-4o:scripted` or `rustybin-echo`.
  - Token counts are deterministic: about one token per four letters or digits, one per other non-space character. Streaming sends one token per delta, so `completion_tokens` equals the number of text deltas. `max_tokens` cuts the reply and sets the "length" finish reason. Stop sequences work.
  - Fit: this is nearly the design we want, already proven in Rust. The AGPL license makes it a design reference for us and a poor dependency.
- axium-lab/llm-mock (https://github.com/axium-lab/llm-mock, README opened). TypeScript on Bun, MIT. Every completion echoes the last user message as "Echo: ..." by default. "Deterministic. Same request, same bytes: ids are content hashes, timestamps derive from them, embeddings are hash-seeded unit vectors." Stateless always. A specific reply rides in the request: header `x-llm-mock-response` gives the text, `x-llm-mock-tool-calls` gives a JSON array of tool calls, `x-llm-mock-status` forces an error. Tool behaviour (search capture of its OpenAI reference page): `tool_choice: "required"` calls the first declared tool, a named tool choice calls that tool, `auto` answers with text ("deciding to call a tool is the one thing a mock cannot do"), and a tool result in the conversation "stops forcing calls, so the loop terminates".
- VidaiMock (https://github.com/vidaiUK/VidaiMock and https://docs.vidai.uk/mock/reference/architecture/, search capture). Rust, Apache 2.0, built on Axum and Tokio. A single binary of about 7 MB, "50,000+ RPS in benchmark mode", and usable as a Rust library inside tests: `vidaimock::MockServer::builder().bind("127.0.0.1:0").start().await`. Every response is a Tera template that can reflect request data. Bundled templates return a tool call when tools are defined, and plain text once a tool result is in the history (a `has_tool_result()` template helper), so agent loops end. Text streams in word chunks and a tool call streams as a single structured chunk. Fit: the only permissively licensed Rust option I found that we could run in-process today. Its replies are template-driven, so "reacts to input" is as good as the templates we write.
- fakellm (https://github.com/1dg618/fakellm, README opened). Python. YAML rules with conditions `messages_contain`, `model_matches`, `tools_include`, `turn`, `turn_in`, `previous_message_role`, `previous_message_contains`, `tool_result_contains` and request headers. First match wins. A rule returns `content`, `tool_calls`, or an error status. With no match "fakellm returns a deterministic echo response derived from a hash of the request". A conversation is identified by "a stable hash of the first user message". This is the finite-state flavour: turn 1 returns a tool call, turn 2 with a matching tool result returns a summary.
- llm-d-inference-sim: echo mode and random mode (section 3).

### Directive-triggered behaviour (the prompt tells the fake what to do)

- simonw/llm-echo (https://github.com/simonw/llm-echo, README opened; source is 176 lines). Apache 2.0. The `echo` model returns the prompt, system prompt, attachments and previous turns as JSON. If the prompt is itself JSON with a `tool_calls` list (`{"name": ..., "arguments": {...}}`), the model emits those tool calls, so "you can use `llm-echo` to test tool calling without needing to run prompts through an actual LLM". After the tool runs, the next echo includes a `tool_results` list. A `raw` key returns an exact string. A `thinking` option emits two canned reasoning chunks first.
- enrico9034/llmock: `[[mock tokens=500 tps=20 seed=42]]` blocks inside the prompt (section 3).
- chabeau-server: single punctuation characters and the words `system` and `raw` (section 1).
- Rustybin scripted mode: trigger words (above).
- elizacp: plans "tool use triggers" inside its ELIZA pattern table (section 1).

### Scripted lists, fixtures and record-replay (for comparison with what we have)

- LangChain (https://docs.langchain.com/oss/python/langchain/test/unit-testing, search capture): `FakeListChatModel` cycles through a list of strings; `GenericFakeChatModel` takes an iterator of messages including tool calls.
- Pydantic AI (opened): `TestModel` "will (by default) call all tools in the agent, then return either plain text or a structured response", with a note that it "isn't doing anything clever to extract values from the prompt". `FunctionModel` replaces the model with your own function of the message list, which is the "pure function of the conversation" design as an API. Setting `ALLOW_MODEL_REQUESTS=False` blocks real calls.
- Vercel AI SDK (https://ai-sdk.dev/docs/ai-sdk-core/testing, search capture): mock language model classes plus `simulateReadableStream` with chunk delays.
- LiteLLM (https://docs.litellm.ai/docs/completion/mock_requests, search capture): `mock_response="..."` on `completion()`, works with `stream=True`.
- CopilotKit aimock (https://github.com/copilotkit/aimock, search capture): fixture matching on `userMessage`, `turnIndex`, `hasToolResult`, `toolResultContains`; record real traffic once and replay; streaming profiles with time to first token and tokens per second.
- OpenAI Codex CLI, written in Rust (search capture of `codex-rs/core/tests/suite/hooks.rs`): tests start a real `wiremock` server and mount canned SSE bodies built from small event constructors (`ev_response_created`, `ev_function_call(call_id, tool_name, args)`, `ev_assistant_message`, `ev_completed`) with `mount_sse_once` and `mount_sse_sequence`. This keeps the HTTP layer, serde and the SSE parser under test. It is scripted, so it is the same class as our current scripted model, just over real HTTP.

### Simulated-user bots

- What I found is fixed scripts, all from search captures: FastAIAgent `SimulatedUser(script=[...])` (https://docs.fastaiagent.net/simulation/), Simulacra `DeterministicConversationGenerator` (https://github.com/kkacquah/simulacra), agentest scripted `turns` (https://github.com/r-prem/agentest), antag `script:` personas (https://github.com/BharZInstein/antag). Each of these offers an LLM persona as the other option.
- I did not find a rule-based or state-machine simulated user that reacts to the agent's replies without an LLM. An ELIZA-style engine pointed the other way (reading assistant turns, emitting user turns) would fill that gap.

### Design patterns worth lifting

- Stateless by construction: derive everything, including ids, from a hash of the request. Same request, same bytes.
- Loop termination rule: while the history holds no tool result, answer with a tool call (always when tools are defined in VidaiMock, when a tool is forced or mentioned in Rustybin, when a tool is forced in llm-mock). Once a `tool` role message is present, answer with text, and in Rustybin's case quote the tool output. Three separate projects landed on this.
- Tool selection without a model: forced tool first, then keyword overlap between the user message and tool names or descriptions.
- Mode selection through the model name, so one endpoint serves echo, scripted and random behaviour without extra headers.
- Deterministic token accounting, one token per SSE delta, so usage numbers and `max_tokens` can be asserted exactly.
- An echo or `raw` command that returns what the fake received, for asserting what the gateway forwarded.

## 6. PyBot itself: announcement, reception, ports and wrappers

The short version: PyBot got a lot of attention on X and almost no technical discussion elsewhere. Within a week people built at least three Rust ports. The best of them is still 30 to 150 ms per reply and hundreds of megabytes of memory. Nobody I found has proposed or built a small distilled version. One project wraps PyBot behind a local chat-completions endpoint as a last-resort backend.

### The project

- Repo: https://github.com/huggingface/pybot (README opened). Created 2026-11-12. License Apache 2.0.
- README facts: "around 640k lines of code that can answer everyday questions, backed by 550k lines of tests the agents wrote themselves". `bot/` holds 528 Python files. Line split: understanding engine 25,000, memory store 109,000, knowledge base 317,000, response composer 104,000, modules 81,000. Each turn every module in `bot/modules/` may propose a reply and `bot/modules/arbiter.py` picks one.
- Speed, from the README: "PyBot takes a few seconds to start, because Python has about 640,000 lines of code to load, and about a second to reply."
- Determinism: option `-s N` ("fix PyBot's choice of wording, to get the same replies again"). Option `-v` shows the module that answered and the time taken.
- Benchmark: win rate on SmolTalkBench "climbed from 0% to 26%", "on par with SmolLM-360M-Instruct". The dataset card (https://huggingface.co/datasets/HuggingFaceSwarms/SmolTalkBench, search capture) says it is 1,000 single-turn everyday prompts judged by `deepseek-ai/DeepSeek-V4.1-Flash`, and warns that SmolLM models "were trained on the source conversations, so the SmolLM baselines' scores are inflated".
- PyPI: https://pypi.org/project/pybot-hf/ (opened). Version 0.1.2 uploaded 2026-11-18, Apache-2.0, Python 3.8 or newer, zero dependencies.
- Browser demo: a Hugging Face Space, HuggingFaceSwarms/PyBot, is linked from the README. I did not open it.

### Announcement and follow-ups

- Lewis Tunstall's post on X, 2026-11-16 (read through a mirror, https://xitter.gq/_lewtun/status/2122333622946373889, opened): 1,000 agents for one week, each "an instance of DeepSeek-V4.1-Flash", about 175k messages on a Slack-like board, about 8k merged PRs, "630k lines of Python, zero dependencies". He adds that "the swarm was blind to the eval" and that they "fed the Instruct-GPT paper to the swarm as a general guide".
- Thomas Wolf's post, 2026-11-17 (search capture of the LinkedIn copy): the task was a benchmark they call "ELIZA-2026", it ran on "a couple of GPU nodes", and in some runs "only 40-60 of the 1,000 agents were actually calling the model". He says the swarm tooling ("flock + msgboard") will be open-sourced and learnings shared "in more detail".
- Message board release: a Hacker News story on 2026-11-18 links https://huggingface.co/spaces/HuggingFaceSwarms/pybot-msgboard ("the largest open dataset of swarm dynamics"). A Digg summary (search capture) quotes 184k messages and 32 million words.
- Hugging Face blog post: Tunstall wrote "We will publish a blog post with all the details". I searched and did not find a published one as of today.

### Discussion

- Hacker News (item page opened, and the Algolia search API queried): three stories and zero comments in total. "Can Python Be a Chatbot?" on 2026-11-16 with 7 points, "PyBot: Can Python Be a Chatbot?" on 2026-11-17 with 3 points (https://news.ycombinator.com/item?id=50528602), and the message board story on 2026-11-18 with 1 point.
- X replies under the announcement (mirror, opened): mostly jokes about expert systems and ELIZA, and questions about swarm coordination. Relevant ones: "rust rewrite?" (answer from Tunstall: "nice idea! time to burn even more tokens :)"), "Would love to see how much of the 630k lines is routing", "How it behaves having access to tools?" (no answer shown), and "Many years ago I did something very similar using AIML." I saw no reply complaining about speed.
- Reddit: an aggregator page (https://hotai.yeuxark.com/a/136549, search capture) lists an r/LocalLLaMA post titled "PyBot: No weights. No training. Pure Python." from 2026-11-16. Reddit itself refused my requests, so the thread's comments are unconfirmed.
- Press-style write-ups (https://www.ayu-sh-kr.com/news/pybot-deepseek-agent-swarm-python/ opened; others as search captures) restate the README, including "a few seconds to start and about a second per reply".

### Ports, wrappers and spin-offs

- hyunhwan-bcm/rsbot (https://github.com/hyunhwan-bcm/rsbot, README opened). "An exact port of PyBot to Rust, written by AI agents (Claude Code) and verified against PyBot itself on every recorded turn." Apache-2.0. Status "complete". Replies identical on 2,198 of 2,198 single-turn prompts, 9,348 of 9,348 conversation turns and 9,000 of 9,000 synthetic turns. It re-judged SmolTalkBench with a different judge and got 26.0% overall. Structure: about 233k lines of PyBot are data (word lists, reply pools, 10k regexes, 8,081 knowledge entries) exported to a 47 MB JSON bundle, and about 152k lines of logic in 10,800 functions were translated. It needs PCRE2 and a re-implementation of CPython's Mersenne Twister to match. Speed, marked preliminary by the author: median 144 ms per reply against 1,129 ms for Python, 90th percentile 382 ms against 4,364 ms, startup plus one reply 0.87 s against 3.06 s, peak memory 904 MB against 180 MB. A WebAssembly build runs in the browser (8.1 MB of code and 9.5 MB of data, compressed) at 0.21 to 0.44 s per reply. The build took about two days and about $1,400 at API list prices. It has a web chat with its own `POST /api/chat` JSON API, which is a custom format and not the OpenAI one. One more useful number: "On real conversations PyBot only runs 42% of its statements".
- Shekswess/rustbot (https://github.com/Shekswess/rustbot, README opened). A native Rust migration: 368,604 lines of Rust in 931 files, Apache 2.0. Benchmark on a Ryzen 7 9700X: reply after warm-up 30.3 ms against 453.7 ms for PyBot, first reply 375 ms against 2.63 s, peak memory 166.6 MiB against 213.7 MiB. Parity is partial: 17 of 20 prompts matched verbatim, and the README says the two "can still choose different wording or route a reply differently".
- azjhf-dev/pybot-rs (https://github.com/azjhf-dev/pybot-rs, README opened). A machine-transpiled port, labelled experimental. "measured first replies take about 25 to 55 seconds, and some multi-turn comparisons time out". It matches on 34 of 38 sampled replies.
- mkieffer1107/RustBot: appears in a GitHub repository search (Rust, no description). Not opened. The owner is the person who asked "rust rewrite?" on X.
- jtakahashi0604/tiny-experiment-202611172200 (README opened): "Compiles the rule-based chatbot pybot into the weights of a Transformer." The opposite direction from what we want.
- linmoxi33/pybot-seed-provenance (https://github.com/linmoxi33/pybot-seed-provenance, README opened). MIT. It replays prompts at several seeds and checks whether the seed changed the wording or the module that answered. On 9 prompts and seeds 0 to 3 every reply was stable. Its timing from PyBot's own `-v` output: median 1,219 ms for the `answers` module and 5,122 ms for `memory_store`. It also points out that `bot/__main__.py` catches every exception and prints "Hello!", so a crashed turn looks like a greeting.
- Svyable/svenbot (https://github.com/Svyable/svenbot, README opened). Apache-2.0, "experimental 0.7 beta". It keeps PyBot as "an optional, pinned upstream submodule" and uses it as the last backend after Ollama and a local OpenAI-compatible server. It serves a local `/v1/chat/completions` with "text-only SSE chunks ending with `data: [DONE]`", described as "a small subset of the OpenAI schema". The README says tools, images and arbitrary system messages "are not implemented". This is the only OpenAI-compatible wrapper around PyBot I found, and it wraps the full PyBot.
- sboghossian/ifbot (https://github.com/sboghossian/ifbot, README opened). Apache-2.0. "A tiny rules-only chatbot" plus a harness that grows it by hill-climbing a benchmark, "inspired by" PyBot and written independently. Modules return `Proposal(text, confidence)`, "the arbiter picks the highest confidence and breaks ties by module name, so the bot is deterministic for a given seed". After two rounds of agent proposals "the bot grew to 2906 lines". This is the nearest thing to a small PyBot, and it shares the architecture, with its own code and data.
- zw2700/pybot-math (GitHub search listing only): "PyBot ... taught grade-school math by Codex agents: 0.9% to 15.2% on GSM8K with no weights".

### What this means for us

- Even the exact Rust port costs about 144 ms median per reply and 0.9 GB of memory, and the lighter native rewrite costs about 30 ms and 167 MiB. Both miss a microsecond-to-few-millisecond budget by one to two orders of magnitude, and both are hundreds of thousands of lines. Porting or wrapping stays off the table.
- The architecture is the reusable part: modules that each propose a reply with a confidence, one arbiter that picks, a small memory of what was said (the README demo remembers the user's name). ifbot shows that shape working at under 3,000 lines.
- Nobody has published a distilled or cut-down PyBot. rsbot's 42% statement-coverage figure suggests a lot of it is rarely reached, but that is my reading, the author did not propose trimming it.

## 7. The borderline option: tiny real models

The short version: Karpathy's 260K-parameter story model is about 1 MB and runs at roughly 650 to 15,000 tokens per second on CPU depending on the machine, so a 50-token reply costs milliseconds to tens of milliseconds. It writes toddler stories and ignores chat roles and tools. SmolLM2-135M is a real chat model at roughly 40 to 150 tokens per second on laptop-class CPUs, so about a second per short reply.

### stories260K (Karpathy tinyllamas)

- Source: https://github.com/karpathy/llama2.c (README opened) and https://huggingface.co/karpathy/tinyllamas/tree/main/stories260K (opened)
- Shape from the README table: dim 64, 5 layers, 8 heads, 4 key/value heads, max context 512, 260K parameters. The tokenizer file is `tok512.bin`, and a third-party write-up (search capture) gives the vocabulary as 512 tokens.
- Size: `stories260K.bin` 1.06 MB, tokenizer `tok512.bin` 6.23 kB. GGUF for llama.cpp at https://huggingface.co/ggml-org/models/tree/main/tinyllamas (opened): `stories260K.gguf` 1.19 MB.
- Speed, llama.cpp: Georgi Gerganov's sample run in the PR that set up the llama.cpp server tests (https://github.com/ggml-org/llama.cpp/pull/5566, search capture): eval "1.06 ms per token, 947.46 tokens per second", prompt eval 2,288 tokens per second, load time 80 ms. He recommended it with "This should be ideal for CI".
- Speed, other machines (search captures): about 650 tokens per second on one Cortex-A55 core of a TV box with llama2.c (https://github.com/hyperionaidev/smolmaxxing/blob/main/EXPERIMENT.md), and 15,032 tokens per second generation on one thread of an Apple M5 Pro with `llama-bench` (https://prachub.com/resources/llama-cpp-interview-questions-gguf-models-gpu-offload-and-reproducible-benchmarks).
- Already used as a test stand-in: the llama.cpp server test suite uses it as the default model, with context 512, 64 predicted tokens and seed 42 (https://github.com/ggml-org/llama.cpp/blob/master/tools/server/tests/utils.py, search capture). The Go project wbrown/tinyoai (https://github.com/wbrown/tinyoai, search capture) embeds it in an OpenAI-compatible `/v1/chat/completions` server with SSE "for testing OpenAI-compatible clients". Its README is candid about limits: "It has no chat training, tool calling, or vision. The server joins message text into a prompt and discards roles."

### stories15M

- Size: "~60MB download" for the fp32 `.bin` (llama2.c README). GGUF sizes: 98.4 MB unquantized, 26.7 MB at Q8_0, 19.1 MB at Q4_0.
- Speed: "On my M1 MacBook Air this runs at ~110 tokens/s" with llama2.c (README).

### hf-internal-testing/tiny-random-gpt2

- URL: https://huggingface.co/hf-internal-testing/tiny-random-gpt2/tree/main (opened)
- Size: `model.safetensors` 454 kB (the older `pytorch_model.bin` is 3.56 MB).
- Nature: as the name says, the weights are random, so output is noise. It exists to test loading and plumbing in the transformers library. No speed is stated on the page. Running it from Rust would mean a transformers-style runtime or a conversion step, which I did not verify.

### SmolLM2-135M-Instruct

- URL: https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct (opened). 135M parameters, Apache 2.0. The card says function calling is supported "for the 1.7B", so the 135M model is a plain chat model.
- Size: about 95 MB at Q4_K_M according to a course handout (search capture). I could not open the official GGUF file listing (HTTP 401).
- Speed on CPU: 39.3 tokens per second with 573 ms to first chunk on an Intel i5-5300U through LM Studio at Q4_K_M (https://metrillm.dev/result/95a24bd5-82c7-4c02-9466-6b2f5fb8e780, opened). 148.6 tokens per second with llama.cpp Q4_0 on 4 AVX2 cores (https://kwker.io/llm/, search capture). 97.6 tokens per second on a Raspberry Pi 5 with 4 threads (https://edgellm-bench.vercel.app/boards/raspberry-pi-5, search capture).
- Quality note from the same metrillm page: structured output scored 3 of 15 and it warns "model produced very low accuracy on quality tasks".

### "No inference" against "almost no inference"

- My arithmetic from the numbers above, for a 50-token reply: stories260K is about 50 ms at 947 tokens per second, or about 3 ms at 15,000 tokens per second, plus model load (80 ms in the llama.cpp sample). SmolLM2-135M is about 0.3 to 1.3 seconds. A rule-based reply is a few regex matches and string formatting.
- What a tiny real model adds: a real tokenizer, real logits, real sampling with a seed, real stop and `max_tokens` behaviour. Useful if we ever test those paths.
- What it lacks for our harness: stories260K ignores roles and cannot be steered to call a tool. SmolLM2-135M can chat but is unreliable at JSON. Neither gives a test author control over what the reply contains.
- Determinism: llama.cpp's own tests pin a seed. Bit-identical output across different CPUs and builds is something I did not confirm.

## 8. Comparing the approaches for the test stand-in job

The job, restated: a Rust fake behind chat-completions that costs microseconds to a few milliseconds, is local and deterministic, reacts to the input and to earlier turns, streams over SSE, and produces tool calls.

### Approach by approach

- ELIZA-style rules
  - Best concrete project: `eliza` crate for the engine (https://github.com/arosspope/eliza-rs), Eliza-GPT for the server design (https://github.com/miguelgrinberg/eliza-gpt)
  - Size: 250 to 1,200 lines of engine, 0.5 to 28 KB of script
  - Reacts to input: yes, through keywords, regex captures and pronoun reflection
  - Reacts to history: yes, through the memory queue and template rotation, rebuilt by replaying user turns
  - Deterministic: yes once the three random picks are seeded or rotated
  - Tool calls: come from a separate layer
  - Fit: strong for the text branch
- AIML, ChatScript, RiveScript, Rasa rules
  - Best concrete project: `rivescript` crate (https://github.com/aichaos/rivescript-rs)
  - Size: a full script-language engine, beta, async with `tokio`
  - Reacts to input and history: yes, with topics and per-user variables held inside the engine
  - Deterministic: yes for single-reply triggers; seeding of random replies is unconfirmed
  - Fit: more engine than the job needs. AIML data is LGPL or GPL. ChatScript is C++ behind FFI. Rasa is a Python ML framework.
- Markov and lorem ipsum
  - Best concrete project: `lipsum` crate (https://github.com/mgeisler/lipsum) for seeded filler; shishberg/llmock (https://github.com/shishberg/llmock) as the reference for "rules first, Markov fallback"
  - Size: tiny engine, table grows with the corpus
  - Reacts to input: only through the seed, or by training on the prompt as llm-markov does
  - Deterministic: yes with a seeded RNG
  - Fit: good fallback and good for long-stream, `max_tokens` and throughput tests. Weak as the only layer.
- Grammar and schema fakers
  - Best concrete project: `noson` (https://crates.io/crates/noson) for schema-valid JSON from a seeded RNG; `tracery` crate for sentence variety
  - Reacts to input: the schema is the input. Rustybin shows how to also pull values from the user text by property name.
  - Deterministic: yes with a seed, or with the fixed recipe (default, first enum value, placeholder per type)
  - Fit: this layer is what makes tool calls and `response_format` work. Required, whichever text approach we pick.
- Deterministic pseudo-model (pure function of the request, plus directives)
  - Best concrete project: Rustybin's mock LLM as the design reference (https://docs.rustybin.dev/ai/behaviour.html, AGPL-3.0); VidaiMock as the usable Rust library (https://github.com/vidaiUK/VidaiMock, Apache 2.0); llm-echo for prompt-embedded tool-call directives (https://github.com/simonw/llm-echo)
  - Size: a reply ladder of a few hundred lines plus the schema generator
  - Reacts to input and history: yes. Tool mention leads to a tool call, a tool result leads to an answer quoting it, a directive leads to exactly what the test asked for.
  - Deterministic: yes by construction
  - Fit: the best match for an agent harness and a gateway
- PyBot and its ports
  - Best concrete project: rsbot (https://github.com/hyunhwan-bcm/rsbot) for fidelity, rustbot for speed, ifbot (https://github.com/sboghossian/ifbot) for the same architecture at 2,900 lines
  - Speed: 30 to 144 ms per reply and 167 to 904 MB of memory for the Rust ports
  - Fit: fails the speed and size goals. Keep only the idea of proposing modules plus an arbiter.
- Tiny real models
  - Best concrete project: stories260K through llama.cpp (https://huggingface.co/ggml-org/models/tree/main/tinyllamas), as llama.cpp's own server tests do
  - Speed: about 1 ms per token on a CI runner by Gerganov's sample
  - Fit: a separate optional tier for tokenizer, sampling and logits paths. It does not follow instructions or call tools.

### Ranking for a small Rust fake

1. A deterministic reply ladder in the Rustybin style, written by us. Order of checks: an explicit directive in the last user message (for example a JSON block or a `call tool X with {...}` line that the fake obeys, as llm-echo and enrico9034/llmock do), then "last message is a tool result, so answer and quote it", then "a tool is forced, or the user text names one, so call it", then "structured output requested, so emit schema-valid JSON", then text. Mode selectable through the model name. Confidence: high - three independent projects converged on this ladder and it is the only design here that exercises tool loops without a script.
2. A schema-driven argument generator under that ladder. Start with the fixed recipe (default, first enum value, placeholder by type, required properties only) and take string values from the user text when a property name matches, as Rustybin does. Use `noson` if we want broad JSON Schema coverage without writing it. Confidence: high for needing the layer, medium for `noson` itself - it is a young crate and I read only its README.
3. A small ELIZA-style rule table for the text branch, replayed over the user turns in the request, so text replies depend on what was said and on earlier turns. Write our own short script aimed at test phrases, and add a "I received N messages, the last user message was X" digest rule as the fallback. Confidence: medium-high - every OpenAI-compatible ELIZA server found uses this replay design, and the engine is a few hundred lines. Using the `eliza` crate directly is medium - it is stable and small but uses unseeded `thread_rng` for fallbacks and an old `rand`.
4. Hash-seeded filler text for length and streaming tests: `lipsum` with a ChaCha RNG seeded from a hash of the conversation, or a chain trained on the prompt's own words. Confidence: medium - simple and deterministic, but it only matters for tests about length, pacing and token limits.
5. Tracery-style grammars for sentence variety. Confidence: low - nice to have, nothing in the test job requires it.
6. An off-the-shelf server in place of our own: VidaiMock in-process. Confidence: medium as a quick start, low as the final answer - it is Apache 2.0 and Rust, but replies come from provider templates, so input and history dependence is whatever we template, and I did not read its source.
7. A tiny real model (stories260K) as an extra tier. Confidence: low for the main job - it is fast enough for CI but cannot be steered, so it tests different things.
8. AIML, ChatScript, RiveScript engines. Confidence in skipping them: medium-high - each brings an engine or a license heavier than the problem.
9. PyBot or any port of it. Confidence in skipping: high - measured 30 ms or more per reply and 167 MB or more of memory in the best port.

### Design notes that fall out of the research

- Keep the fake stateless and derive the reply, the ids and any random seed from a hash of the request. That gives determinism and lets one fake serve parallel tests.
- History dependence comes from replaying the `messages` array, which every stateless ELIZA server here already does.
- Count tokens with a fixed rule and send one token per SSE delta, so usage and `max_tokens` assertions are exact.
- Give tests an echo mode that returns the rendered request, so a gateway test can assert what was forwarded.
- Split tool-call arguments across several deltas when streaming, since that is the path real providers exercise. VidaiMock and Rustybin differ here: VidaiMock sends a tool call as a single structured chunk, Rustybin uses each provider's native argument deltas.

## 9. What I could not confirm

- Speed of any ELIZA or Markov engine in microseconds: no project states one. The expectation that they fit the budget is mine.
- The Reddit r/LocalLLaMA thread about PyBot: Reddit refused my requests, so I know only its title from an aggregator.
- A Hugging Face blog post about PyBot: promised on X, not found.
- The Hugging Face Space for PyBot and the message-board Space: linked, not opened.
- The current location of the TypeScript `@elizaos/plugin-eliza-classic` README: seen through the search tool, but the GitHub path returned 404 when I fetched it. The Rust crate page on crates.io is confirmed.
- ElizAPI streaming support.
- shishberg/llmock's license (the README only points at a LICENSE file) and fakellm's license (same).
- Whether the `markov`, `markovify-rs` and `markov_strings` crates accept a seeded RNG, and whether `rivescript` exposes a seed for its random replies.
- AIML's `that` and `topic` behaviour: stated from general knowledge, no page opened.
- `schemars`-based fake data generation: nothing found.
- The official GGUF sizes for SmolLM2-135M-Instruct (file listing returned 401), and any CPU speed for `tiny-random-gpt2`.
- Bit-identical determinism of tiny models across machines.
- Source-level details of VidaiMock, Rustybin, `noson`, `rivescript` and `elizaos-plugin-eliza-classic`: I read documentation and READMEs for these, not code. I did read source for eliza-rs, Eliza-GPT's `v1.py`, llm-markov, and parts of llm-echo and chabeau-server.
- A rule-based simulated user that reacts to agent output without an LLM: none found, only fixed scripts.
- Anyone proposing a distilled or cut-down PyBot: none found.
