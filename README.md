# Codex-external maintenance

This branch contains only GitHub-hosted update automation. Source is on
`external/0.154.0-alpha.6.2`. The updater rebases that patch series onto newer
official Rust release tags and qualifies candidates on macOS before publishing
`external-qualified/<release>` and moving `external/current`.

Failures do not replace the previous qualified branch. The first run must pass
before `external/current` exists. Artifacts are experimental Rust binaries, not
redistributable Desktop UI packages. Live provider authentication and Desktop
UI interchangeability still require local tests; CI never receives those keys.

GitHub scheduled workflows may be delayed and can be disabled after repository
inactivity. This is best-effort updating, not a guarantee that every upstream
release can merge without intervention. Manual runs are available in Actions.
