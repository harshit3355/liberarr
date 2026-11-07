# Liberarr

A single CLI that frees your Plaud recordings from the vendor: transcript, French summary, mind map and action table, with the AI provider of your choice, exported to Obsidian and browsable in a local web UI.

> **Status: v2 rewrite in progress.** The Python v1 was removed; this branch holds the Rust workspace skeleton. Nothing is usable yet.

Liberarr is an independent project, not affiliated with Plaud. MIT licensed (see [LICENSE](LICENSE), [NOTICE](NOTICE)).

## Development

Requirements: Rust (version pinned in `rust-toolchain.toml`) and Node (build-time only, for the web UI).

```sh
npm --prefix web ci && npm --prefix web run build   # builds web/dist, embedded into the binary
cargo build --release
./target/release/liberarr --version
```

Rules for contributors and AI agents: see [AGENTS.md](AGENTS.md).
