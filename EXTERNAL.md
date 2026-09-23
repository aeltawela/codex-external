# Codex-external

Personal experimental build of Codex with the cross-provider agent patch from
[Bozentan/codex PR 2](https://github.com/Bozentan/codex/pull/2).

## Source lineage

- Current upstream: `rust-v0.155.0-alpha.16.3`, matching the backend bundled
  with Desktop `26.917.62051` (build `10789`).
- Current branch: `external/0.155.0-alpha.16.3`.
- Previous upstream: `rust-v0.154.0-alpha.6.2` (`b5bffd3ec4db487e7e3dec59663875b0ef7b72ca`).
- Original patch: `af3072ebbcde8d47bb8592a6a3226ef1b83b560a`.
- Rebased patch: `124101b30b` on `external/0.154.0-alpha.6.2`.
- The September 23 rebase retains the fourteen fork commits through
  `5cf5a97e6`. Provider-aware child configuration now uses upstream's
  `agent/child_config` module, message delivery uses upstream's shared delivery
  path, and account visibility integrates with workspace-routing discovery.
- Qualification results below are dated historical results, not certification
  of later rebases.

### September 23, 2026 qualification

The Desktop-matched `0.155.0-alpha.16.3` rebase passed 987 focused tests across
core, app-server, agent roles, provider/model management, memories, and TUI.
Two thinking-catalog tests also passed. Scoped Clippy, formatting, configuration
schema generation, and Bazel lock refresh completed successfully. The CLI and
code-mode companion were built on Apple Silicon with Rust 1.95.0; the companion
uses checksum-verified Codex V8 release artifacts.

The first test pass exposed two compatibility defects now corrected: role-locked
external spawns must bypass parent model defaults, and provider-owned catalog
authentication must not depend on ambient ChatGPT login. The latter is covered
both signed in and signed out, including absence of the ChatGPT account header.

An isolated app-server using the local external configuration initialized and
listed 16 models, including nine external model IDs. The experimental protocol
schema matched the official Desktop build 10789 backend exactly. The history
helper policy check passed with `--ignore-user-config`. These startup checks
sent no inference requests and did not open existing task storage. Native UI
interaction and fresh paid-provider inference were not part of this qualification.
- Rebase resolution retains both the upstream token-budget startup module and
  the patch's subagent-provider module and exports.
- The release tag's Cargo.lock still used `0.0.0` for workspace packages;
  Cargo refreshed those workspace versions without updating registry packages.

## Compatibility contract

The normal CLI and official Desktop bundle must remain unchanged. The external
CLI command will be `codex-external`. A Desktop variant must be a separate app
bundle; its proprietary UI cannot be rebuilt from this repository. Replacing
its bundled Rust core is experimental and must be qualified independently.

External agents use a distinct plaintext tool surface with explicit provider
authorization. This does not decrypt OpenAI state. Cross-provider children must
start fresh; encrypted OpenAI history is not portable to other providers.
Side-by-side availability does not imply that the same active task can be
edited concurrently in both apps, or that all histories are cross-provider
resumable. Shared-task compatibility requires runtime verification.

## Qualification

### CLI mixed-provider resume picker

When model routing is configured, the CLI resume/fork picker leaves the provider
filter to the app-server's mixed catalog instead of pinning it to the current
default provider. Cwd, archive, and interactive-source filters remain intact.
This changes task discovery only, not history portability or resume safeguards.

### Side-chat provider inheritance

With model routing enabled, a fork that omits both model and provider inherits
the parent's stored selection. Desktop side chats use this request shape; they
must not apply a newly changed global default to an existing task's history.
Explicit model/provider selections (including config overrides) retain normal
routing and cross-provider history checks. This does not make OpenAI history
portable to external models or disable the automatic-helper policy.

### Optional OpenAI background inference

Set `allow_automatic_openai_inference = false` in the external profile. The
external launcher additionally exports
`CODEX_DISABLE_AUTOMATIC_OPENAI_INFERENCE=1` before launching the backend, as
the default when no explicit policy setting exists. Computer History invokes
`exec --ignore-user-config`, and root-level `-c` options can be lost when a CLI
subcommand has its own overrides. The inherited default covers both cases.
Without this environment variable or a config setting, upstream behavior
remains unchanged. The normal launcher and official installation are untouched.

When disabled, resolved OpenAI-provider ephemeral feature sessions (including
forked helpers), memory consolidation sessions, and the dedicated
`openai-memgen` history provider are rejected before session initialization.
Optional memory-generation startup jobs using OpenAI are skipped too. Normal
user chats and explicitly selected subagents remain available, including GPT.
External-provider helpers keep their existing provider; unsupported helpers are
skipped rather than silently rerouted. In particular, a blocked ambient-safety
classifier must not be replaced with an unverified classifier or bypassed.

This policy also skips OpenAI helpers accompanying a deliberately selected GPT
chat in the external app. To opt back in, explicitly pass
`-c allow_automatic_openai_inference=true` after the subcommand (or set it in
the external profile). Account login, connector credentials,
existing memories, and Computer History capture settings are not changed.
The policy applies to new helper launches through this backend, not already
running sessions or an independent official-app history process. Restart the
external app to load an updated backend. This is not an account-wide spending
cap, and does not claim that all background requests are billed credits.

Qualification on 2026-09-19: 515 focused checks passed across app-server,
configuration, provider/history routing, memory admission, and TUI recaps.
The new regression reproduced unwanted OpenAI helper admission before the fix.
Both config-file and launcher-environment policies are covered, including the
routed provider, forked helpers, the history provider, explicit
GPT chats/subagents, external helpers, and explicit policy opt-in without
issuing inference. The user's recap-attempt cap remains unchanged.
For an installed-launcher smoke check with an isolated home and loopback
endpoint, run `node scripts/check-automatic-openai-policy.mjs /path/to/codex-external`.

Mixed-catalog app-server chat lists default to the current provider plus all
providers in `model_provider_routes`, so a remote client omitting the provider
filter does not hide the other catalog's chats. Explicit filters, empty-filter
all-provider requests, and single-provider defaults keep their existing behavior.
This only changes listing; provider switching and history safeguards are unchanged.

The mixed parent model catalog now uses `model_catalog_json` together with a
`model_provider_routes` map from model ID to configured provider ID. Routes
apply when creating or reopening a session, including app-server `thread/start`.
Unsupported inherited effort falls back to the selected model's catalog default.
Mapped external models disable the unsupported hosted web-search declaration.
Existing live sessions cannot change providers. Reopening external history on
OpenAI is supported; returning OpenAI history to external models is rejected
before inference. Same-provider opaque reasoning is preserved on resume: an
`encrypted_content` field alone does not identify OpenAI state. External resume
requires matching session provenance and all historical turn models mapped to
that provider; unknown or foreign historical models are rejected. This is not
encryption/decryption support.

Picker qualification on 2026-09-14: 536 focused core unit tests and eight
provider/history integration tests passed, plus the app-server picker routing
test. Scoped Clippy, repository formatting, and the CLI build passed. All five
skill models returned live answers through app-server: DeepSeek V4.1 Flash,
GLM 5.3 Flash, GLM 5.3, Kimi K3, and Gemma 4 31B. Live CLI checks passed for
external creation, same-provider resume, external-to-OpenAI resume, and rejection
of a return to external after OpenAI history. The installed wrapper also returned
a live DeepSeek answer. These checks do not verify every thinking tier's effect.

The CLI and code-mode-host binaries build with Rust 1.95.0. The initial focused
provider suite passed 17/17 tests, and the app-server suite passed 17/17.
A live OpenAI-parent/Ollama-child run exposed an inherited hosted web-search
tool unsupported by Ollama. A failing regression test proved that role files
could not disable that tool. The added bounded role override fixes it without
allowing roles to enable web access forbidden by the parent. All 42 selected
role/provider unit tests then passed, and a fresh live delegation returned the
same marker from both the Ollama child and OpenAI parent.

A shared-history test resumed the same OpenAI task through external, official,
and external CLI versions in sequence. The Desktop UI initialized and connected
to the custom app-server. Visual Desktop interaction remains unverified because
the UI-control tool blocks Codex windows. These checks are not a full-suite or
all-provider certification.

Use the repository-pinned Rust toolchain from `codex-rs/`. On low-disk hosts,
disable incremental compilation and debug symbols. Run focused tests through
the repository's `just test` recipe, not direct `cargo test`.

## JSON patch compatibility

External providers can return `apply_patch` as a JSON function call with an
`input` string instead of the freeform custom call. The old handler rejected
that payload kind as fatal before returning a tool result; an isolated regression
reproduced the resulting stalled turn. The handler now accepts both forms through
the same parser, permission checks, hooks, and executor. Malformed JSON produces
a recoverable model-facing error, and hook rewrites preserve the function-call
response pairing. This is unrelated to encryption or provider-history routing.

Qualification on 2026-09-14: 114 focused patch and cross-provider checks passed,
including a fresh external child applying a JSON patch and completing its turn.
Scoped Clippy, repository formatting, and the CLI build passed. A live OpenAI
parent spawned a DeepSeek Cloud child which directly added and updated a file
using JSON patch calls and returned its final result. The live run also verified
recovery from a malformed argument, and a separate read-only session correctly
denied the edit without hanging. Existing running Desktop backends
need a restart when idle to load a replacement binary; this does not repair
already-stalled turns automatically.

## Maintenance

### Desktop account services and browser control

With a mixed provider catalog, `getAuthStatus` and `account/read` retain the
existing OpenAI account identity when an external inference model is selected.
The external provider still does not require OpenAI authentication: signed-out
external use remains possible, and inference continues to use only the external
provider's credentials. Single-provider behavior and Bedrock's provider-owned
account representation are preserved. Token export and permanent-refresh-failure
safeguards are unchanged. This addresses Desktop plugin-connection requests made
without an access token after selecting an Ollama model.

Qualification on 2026-09-17: all 67 app-server account/authentication tests passed,
including mixed-catalog identity visibility, signed-out and single-provider
behavior, external inference credential isolation, and active/saved Bedrock
credential cases. The regressions were observed failing before their fixes.
Scoped Clippy, formatting, and the CLI build passed. Live backend reads retained
the ChatGPT account/token with both GPT and DeepSeek selected, without making an
inference request. A direct connector HTTP probe returned 403 under both models;
the plugin connection UI still needs a retry in the restarted Desktop app.
This is not an end-to-end plugin certification.

Run account regression checks without inheriting the launcher's user-config
override: `env -u CODEX_APP_SERVER_TEST_USER_CONFIG_FILE just test -p codex-app-server -E 'test(suite::auth::) | test(suite::v2::account::)'`.

Browser control is **not supported by this locally built Desktop backend** in
Desktop 26.908.70816 (9275). The packaged native browser bridge authenticates the
socket peer and its parent/grandparent code-signing identities. The locally built
backend has an ad-hoc signature, so it produces `missing-code-signing-identity`
even with the unchanged, officially signed Node helpers. Replacing only the
code-mode companion cannot satisfy that ancestry check. The official signed
backend is required for that trusted integration; using it also removes this
fork's native external-provider routing from Desktop. Do not disable or spoof
peer verification. A successful inference or picker test does not qualify browser
control or every Desktop plugin/tool integration.

### Desktop 26.908.70816 (9275)

The separate signed UI copy was refreshed from the installed official release.
It still bundles core `0.154.0-alpha.6.2`; experimental app-server JSON schemas
match the previous UI bundle and the patched external core exactly, so no core
rebase was required. The external launcher keeps its name and points to the
versioned UI copy while retaining the patched CLI, separate Electron data, shared
Codex home, and disabled in-place UI updater. The prior UI is retained for rollback.
The version-pinned picker characterization supports both UI asset sets; both
pass the Max/None visibility checks. Runtime UI activation requires reopening the
external launcher. This is not an end-to-end remote-connection certification.

### Thinking catalog and Desktop visibility (2026-09-15)

`scripts/external-thinking.mjs` updates only the five external catalog entries:
DeepSeek V4.1 Flash, GLM 5.3 Flash, GLM 5.3, and Kimi K3 expose low/high/max.
Gemma 4 31B exposes none/high as thinking off/on, not three native depth tiers.
The configured High defaults remain; Gemma's former Low default becomes High
to represent the qualified thinking-on setting. Existing agent role files are
not changed by this catalog transformer.

All 14 combinations completed a small live Ollama Responses request. The
installed app-server also sent every exact selected model/effort pair to a local
fake provider. These are compatibility checks, not a reasoning-quality benchmark.

The signed Desktop UI accepts the names `low`, `high`, and `max`; it labels Low
as Light. Its separate `enabled-reasoning-efforts` preference defaults to
low/medium/high/xhigh/ultra/persistent, filtering out Max and None even when the
backend advertises them. `scripts/inspect-desktop-thinking.mjs` characterizes the
actual bundled pure filter without launching or modifying the UI. Adding Max and
None to that preference preserves those options. This preference is stored in
the shared Codex home; changing it affects both apps' picker visibility, not
their selected model/effort. Such a shared change requires separate approval.

Sources: [GLM Flash](https://ollama.com/library/glm-5.3-flash),
[GLM](https://ollama.com/library/glm-5.3),
[Kimi](https://github.com/MoonshotAI/Kimi-K3),
[DeepSeek](https://api-docs.deepseek.com/api/create-response/),
[Gemma](https://ollama.com/library/gemma4:31b-cloud).

### Rebase qualification

A GitHub Actions workflow is active on a separate maintenance branch. It
checks upstream releases daily, tests candidate rebases before promotion, and
preserves the previous qualified branch on failure. Run 34795647962 passed its
build/tests but GitHub rejected branch publication with a workflow-scope/timeout
error. Automatic promotion is not qualified. No ChatGPT scheduled task is installed.
GitHub scheduling is best-effort and does not replace the
official application's normal updater or automatically modify this Mac.

Never commit authentication, local histories, or generated private configuration
to this repository. Add provider models and thinking controls only after the
native external-agent route has passed a live qualification.
