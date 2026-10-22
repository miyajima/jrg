# jrg selection in RTK Proxy

This standalone, optional binary adds a `jrg-web-selection` `Transform` to
RTK Proxy's public `Pipeline`, then serves RTK's `server::build_router` behind
an Anthropic-only route guard. It does not replace an existing RTK daemon.
No user configuration files, transcript files, databases or API keys are read
at startup. It relays credentials supplied by the HTTP caller.

The dependency is pinned to RTK Proxy v1.0.1, commit
`4ef8730cf3a0dea11c8d64a9b90bc0616fe52100`. RTK Proxy is licensed under
BUSL-1.1; see the pinned upstream `proxy/LICENSE` and its Additional Use Grant
before distributing or offering this binary. This directory is a separate Cargo
workspace because RTK and jrg currently require incompatible SQLite link versions.
It calls the existing `jrg-select` executable through bounded JSON stdin/stdout.

## Build and opt-in run

From the jrg root, build the selector first:

```sh
cargo build --bin jrg-select
cargo build --manifest-path integrations/rtk-proxy/Cargo.toml --locked
```

Run only when intentionally testing a caller against this separate proxy:

```sh
integrations/rtk-proxy/target/debug/jrg-rtk-proxy \
  --selector /absolute/path/to/jrg/target/debug/jrg-select \
  --anthropic-upstream http://127.0.0.1:9000 \
  --bind 127.0.0.1:8081 \
  --search-tool search --page-tool fetch \
  --max-tokens 1000 --selection-timeout-secs 10
```

`--anthropic-upstream` is required; it has no production default. The bind must
be a loopback IP socket address. Only `POST /v1/messages` and `GET /health` are
exposed. Chat Completions, Responses, admin and fallback passthrough routes return
404. No OpenAI/Responses selection is implemented. No real client base URL is
changed by building, testing or starting this program.

## Supported tool contract

Configure exact tool names explicitly with repeatable `--search-tool` and
`--page-tool`; omitted names default to `search` and `fetch`. Search and page tool
names must not overlap. Other tools with valid preceding origins pass unchanged.

The selector always receives `--query-rules verified`. A verified registry rule
applies only to an approved complete normalized request, including primary and
English focus, token budget and documents, with the matching catalog/engine
fingerprints. Requests with no matching approved profile use rules `Off`.
This proxy never enables the whole built-in rule catalog and adds no per-call
profile argument. Its memo key includes the verified policy and selector hash.

Each Anthropic `tool_result.tool_use_id` is joined to its preceding assistant
`tool_use.id`. The input focus uses the first nonempty string among
`jrg_focus`, `focus`, `query`, `q`, otherwise the real user text preceding that
originating call. Tool result text is never treated as a user question. A later
user question cannot alter an earlier tool call's focus.

A caller may explicitly support `tool_use.input.jrg_focus_en` as optional
selection metadata, for example
`{"jrg_focus":"公開日","jrg_focus_en":"Aurora"}`. The originating English
string is frozen with the primary focus and sent to `jrg-select` as
`query_expansions: ["Aurora"]`; the primary `query` remains unchanged. Omitting
the field preserves the legacy selector envelope. A provided value must be a
nonblank string. The trimmed primary and English expansion, including their
newline separator, must fit within 4096 UTF-8 bytes. Invalid metadata becomes
an explicit error result without forwarding the raw source.

The shared selector also applies the versioned, source-independent
[frequent-expression catalog](../../data/QUERY_EXPANSIONS.md) to the question
and supplied English. Generate English before reading the tool output, preserving
intent and using unambiguous API names and equivalent numeric bounds without
adding answers. Catalog updates require rebuilding the selector and restarting
the proxy; do not replace the selector binary while a proxy has memoized results.

For mapped tools that already declare
`input_schema.properties.jrg_focus_en` with `type: "string"`, the proxy updates
only its description with the same source-blind guidance as the MCP adapter:
generate English from the user's task before reading tool output, preserve
negation, versions, numbers and identifiers, and include unambiguous standard API
terms and equivalent numerical bounds without adding facts or assumed settings.
Other tool definitions and schema constraints remain intact. Absent fields and
fields with other types are never added or changed.

The caller or orchestrator must accept this selection-only argument and remove
it from the arguments sent to the actual tool provider, while retaining it in
the originating `tool_use` history. RTK does not inject tool definitions or
system instructions, and cannot strip an extra argument before the client's
tool validator runs. The separate MCP adapter owns its schema and stripping
path. This proxy does not generate a translation or call another model.

Result `content` must be either a JSON string or an array with exactly one
`{"type":"text","text":"JSON"}` block. The decoded payload is:

```json
{"results":[{"id":"optional","url":"https://example.test/","title":"Title","text":"Original snippet"}]}
```

for search, or:

```json
{"id":"page","url":"https://example.test/","title":"Title","markdown":"# Heading\n\nOriginal text"}
```

for a page. Original URLs, text, block IDs and SHA-256 provenance come from
`jrg-select`; this transform does not rewrite the input source for citations.
Selected content includes the RTK truncation marker and explicit omission counts.
Selection is lossy evidence retrieval; omission counts are not factual summaries.

Configured tool errors remain `is_error: true`. Missing/ambiguous origins, empty
focus, malformed or mixed-media content, oversized input/output, selector exit,
invalid selector response and timeout become short explicit error results with no
raw fallback. Successful configured results are reconstructed, so extra metadata
cannot retain a mirrored original payload. The transform returns `Ok` after
replacing these failures because RTK forwards the original body on transform
`Err`.

Each selector input and stdout is capped at 4 MiB. Stderr is discarded, process
arguments are passed directly, and timeout/overflow kills and reaps the child.
On Unix each selector has a new process group. Cleanup sends one `SIGKILL` to
that group on timeout, overflow, normal completion or request-future drop, so a
wrapper cannot leave ordinary subprocesses running. Failures and cancellation
signal before the leader is reaped. Normal completion uses Tokio's child wait
and immediately signals the group without an intervening await; surviving
descendants retain its ID. With an already-empty group there is a small ID-reuse
window between reap and signal. No polling or later retry widens that window.
Subprocesses that deliberately leave the group are outside this cleanup scope.
Other platforms retain direct-child cleanup; process-group checks run on Unix.
The in-memory memo stores only selected/error replacements and hashed keys;
it retains at most 128 entries and 8 MiB of replacement JSON. The transform pins
the selector's canonical absolute path and complete SHA-256 at creation. Library
callers may also supply a bare executable name, resolved once through absolute
`PATH` entries; relative paths with directories are rejected. The CLI continues
to require an absolute path. Executable hashing is bounded to 128 MiB.

Before every configured selection, including memo reuse, the pinned executable
is rehashed. Replacement, modification or loss produces an explicit
`selector_changed` error; an initially unavailable executable cannot hot-load.
Restart after rebuilding the selector or changing tool names, focus rules or
token policy. PATH and configured symlink changes never repin a running
transform. This retains one selector identity for the conversation. The memo is
an optimization, not durable storage; deterministic selection and the originating
focus keep results stable after eviction/restart when the same selector and
policy are used.

Token attribution uses RTK's `o200k_base` estimate. It is not an Anthropic bill,
a provider prompt-cache hit, or proof of cost/quality improvement. No live
provider request is needed for the tests.

## Functional verification

```sh
cargo test --manifest-path integrations/rtk-proxy/Cargo.toml --locked
```

Tests use the local selector, synthetic messages and credentials, and loopback
HTTP upstreams. They verify original citation text, omitted search noise,
Markdown table/footnote context, origin-bound historical focus, reused tool IDs,
explicit failures, passthrough of other tools, and real RTK forwarding/response
relay. They do not prove live provider compatibility, production cache behavior
or economic benefit.

Local verification on 2026-10-22 used a task-local Cargo home
`/private/tmp/jrg-rtk-cargo-home` and the root `target/debug/jrg-select`:

```sh
CARGO_HOME=/private/tmp/jrg-rtk-cargo-home CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true cargo test --locked
# 26 passed; unit, binary, integration and doc-test harnesses completed.
CARGO_HOME=/private/tmp/jrg-rtk-cargo-home CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true cargo clippy --all-targets --locked -- -Dwarnings
# No issues found.
CARGO_HOME=/private/tmp/jrg-rtk-cargo-home cargo fmt --check
# Exit 0.
```

The initial no-op transform failed 7 of the original 9 tests, including a
`RAW_NOISE` sentinel observed at the fake HTTP upstream; the same tests pass
with selection enabled. A tenth test launches the compiled custom binary and
verifies the complete local HTTP round trip and non-loopback bind rejection.
Four further descendant regressions failed before the cleanup fix and pass with
isolated process groups: timeout, output overflow, request cancellation and a
leader that exits first. Each also verifies that an unrelated process survives.
Five bilingual-focus tests cover Japanese focus with explicit English evidence
terms, omitted-field compatibility, invalid metadata and the UTF-8 byte boundary,
historical stability, and distinct memo entries when only English focus changes.
Seven verified-policy tests cover explicit `verified` argv, existing-field-only
English guidance, an executable changed with its original size/mtime restored,
an initially missing executable, safe PATH/relative-path behavior, and actual RTK
HTTP forwarding with unchanged originating English. The five new behavior tests
failed before implementation; the two additional path boundary tests pass.
The proxy's argv and forwarding tests do not approve registry profiles or prove
blanket rule equivalence; rule approvals remain scoped to core registry evidence.
These are local functional checks only; no installed daemon, real client
configuration or live provider was changed.
