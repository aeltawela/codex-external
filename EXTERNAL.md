# Codex-external

Personal experimental build of Codex with the cross-provider agent patch from
[Bozentan/codex PR 2](https://github.com/Bozentan/codex/pull/2).

## Source lineage

- Upstream: `rust-v0.154.0-alpha.6.2` (`b5bffd3ec4db487e7e3dec59663875b0ef7b72ca`).
- Original patch: `af3072ebbcde8d47bb8592a6a3226ef1b83b560a`.
- Rebased patch: `124101b30b` on `external/0.154.0-alpha.6.2`.
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

The mixed parent model catalog now uses `model_catalog_json` together with a
`model_provider_routes` map from model ID to configured provider ID. Routes
apply when creating or reopening a session, including app-server `thread/start`.
Unsupported inherited effort falls back to the selected model's catalog default.
Mapped external models disable the unsupported hosted web-search declaration.
Existing live sessions cannot change providers. Reopening external history on
OpenAI is supported; returning OpenAI or opaque provider state to external
models is rejected before inference. This is not encryption/decryption support.

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

## Updates

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
