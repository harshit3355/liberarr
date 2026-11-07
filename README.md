# Liberarr

A single CLI that frees your Plaud recordings from the vendor. Audio comes straight off the device over Bluetooth (or from local files); Liberarr produces a transcript, a French summary, a mind map and an action table with the AI provider of your choice, exports notes to Obsidian, and serves a local web UI. One binary, nothing else to install.

Liberarr is an independent project, **not affiliated with, endorsed by or sponsored by Plaud**. "Plaud" is used only to describe the hardware it works with.

## Install

Download `liberarr-<target>.tar.gz` for your platform from the [releases page](../../releases) (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`), check it against the `.sha256` file, and unpack it:

```sh
shasum -a 256 -c liberarr-<target>.tar.gz.sha256
tar -xzf liberarr-<target>.tar.gz
chmod +x liberarr
xattr -d com.apple.quarantine liberarr 2>/dev/null || true   # macOS: the binary is not notarized
mkdir -p ~/.local/bin && mv liberarr ~/.local/bin/           # then make sure ~/.local/bin is in your PATH
liberarr --version
```

Linux builds need glibc 2.35 or newer (Ubuntu 22.04, Debian 12) and use the host's BlueZ service for Bluetooth (present on desktop distributions); `libdbus` is linked dynamically.

## Quick start

```sh
liberarr init    # creates ~/.liberarr/.env: generated secrets, asks for your AI provider keys
liberarr link    # one-time: fetches the device keys from your Plaud account, then tests an unlock
liberarr pull    # pulls new recordings off the device, then processes them
liberarr serve   # web UI on http://127.0.0.1:8484
```

| Command | Purpose |
|---|---|
| `init` | Create the home folder and `.env`. Idempotent: values already set are kept. Tests each key and proposes models. `--non-interactive` reads `LIBERARR_{TRANSCRIBE,LLM}_{PROVIDER,BASE_URL,API_KEY,MODEL}` from the environment. Prints the generated admin password once. |
| `link [--force]` | Paste your Plaud web-app token (hidden prompt). To find it: log in at web.plaud.ai, then in the browser DevTools open Application → Cookies and copy the `pld_ut` value. The token is used for this run only and never stored. Fetches the device keys; a key pair already stored is only replaced with `--force`. |
| `pull [--no-process]` | Wake the device and close the Plaud app on your phone first. Resumes dropped downloads, never pulls the same recording twice, skips a recording still in progress. |
| `process <files…>` | Run the pipeline on local audio files. |
| `serve [--host] [--port]` | Web UI and API, with the job worker. |
| `status` | Known devices, last pull, queued and failed work. |

## Configuration

Everything lives in `LIBERARR_HOME` (default `~/.liberarr`): `.env`, `liberarr.db`, `audio/`, `device/`, `obsidian/`.

- **`LIBERARR_HOME` must be on a local disk.** The job worker lock uses advisory file locks, which are unreliable on network filesystems and some Docker bind mounts.
- **`<LIBERARR_HOME>/.env` is the only configuration source** (mode 0600, written by `init`). Shell variables are ignored; the one exception is `LIBERARR_HOME` itself, which locates the file. Liberarr refuses to start when a secret is missing or still a placeholder, naming the variable but never its value.
- Optional settings, not written by `init` (add them to `.env`):
  - `LIBERARR_OBSIDIAN_DIR`: folder (for example a vault) for the Obsidian export; default `<LIBERARR_HOME>/obsidian`. The export is one-way and only writes between `%% liberarr:start %%` and `%% liberarr:end %%`.
  - `LIBERARR_COOKIE_SECURE=true`: set the `Secure` flag on the login cookie when serving over HTTPS.
  - `LIBERARR_LANG`: language of the CLI messages (`en` or `fr`); default: the OS locale.
- `LIBERARR_CONTENT_LANGUAGE` (`en` or `fr`): language of the summaries and note headings. `init` asks for it and writes it, so notes keep one language whatever the locale.

## AI providers

Two profiles in `.env`, each with its own `BASE_URL`, `API_KEY` and `MODEL`: `LIBERARR_TRANSCRIBE_*` (speech to text) and `LIBERARR_LLM_*` (summary). Any OpenAI-compatible endpoint works:

- **Transcription**: OpenAI (`whisper-1`), Groq (Whisper models), or any endpoint that serves `/audio/transcriptions` with `response_format=verbose_json`. Gemini has no such endpoint. OpenAI `gpt-4o-transcribe` / `gpt-4o-mini-transcribe` are not supported in v2.
- **Summary**: OpenAI, Groq, Gemini (OpenAI-compatible base URL), or Claude through a gateway.
- **Claude**: through an OpenAI-compatible gateway (for example LiteLLM). For the **summary only**: Claude does not transcribe audio, so keep a speech-to-text provider on the transcribe profile.

- **OpenRouter** (`openrouter`, summary only): OpenRouter's OpenAI-compatible API, with an optional restriction to your own provider keys (BYOK, for example Azure). Settings: `LIBERARR_LLM_BASE_URL` (`https://openrouter.ai/api/v1`), `LIBERARR_LLM_API_KEY` and, optionally, `LIBERARR_LLM_ROUTING_ONLY`, a comma-separated list of provider names (such as `azure`). With a list, every request is restricted to those providers, with no fallback. Without it (missing or empty), OpenRouter may serve requests with its own shared models. `init` tests the key but lists no models: type the model id (such as `openai/gpt-6-luna`); to clear the list when reconfiguring, answer `-`. To be sure only your own key is used, also set the key's "Shared capacity fallback" to "Never use shared capacity for any model on this provider" in your OpenRouter BYOK settings.

Transcription forces no language, so French mixed with English works. Summaries are in French. Audio above 25 MB is split without ffmpeg only for Ogg (the device format); other formats above 25 MB are refused.

## Web UI and network exposure

`serve` listens on `127.0.0.1:8484` by default. One admin user (`admin`, password in `.env`), login rate limit, and an API key. To reach it from your LAN or a NAS, pass `--host 0.0.0.0` and put it behind a reverse proxy or VPN; under HTTPS set `LIBERARR_COOKIE_SECURE=true`. `serve` never talks to the device: run `pull` separately.

## Hybrid mode with the Plaud app

Liberarr and the Plaud phone app can coexist, with these rules:

- Keep the app **only for firmware updates**.
- Turn **off "Sync in idle"** in the app, and do not sync recordings through the app.
- Apply a firmware update only after checking its release notes, then verify `liberarr pull` still works.
- Close the app on your phone before `pull` or `link`: the device talks to one Bluetooth client at a time.

## Privacy

- **Every recording is sent to the AI providers you configure** (audio to the transcribe provider, transcript to the LLM provider). There is no per-type routing in v2. Do not use a provider you would not trust with a work meeting.
- `link` fetches the device keys from your Plaud account once. Plaud keeps its own copy of the key pair; Liberarr cannot change that.
- Device keys are stored encrypted at rest under a key from `.env`. Transcripts, audio, tokens, keys and serials are never logged.

## Development

Requirements: Rust (version pinned in `rust-toolchain.toml`) and Node (build-time only, for the web UI). On Linux also `libdbus-1-dev` and `pkg-config`.

```sh
npm --prefix web ci && npm --prefix web run build   # builds web/dist, embedded into the binary
cargo build --release
./target/release/liberarr --version
```

Contributors can also use `cargo install --path crates/liberarr --locked` after the web build. Rules for contributors and AI agents: see [AGENTS.md](AGENTS.md).

## License

MIT, see [LICENSE](LICENSE). Protocol credits in [NOTICE](NOTICE).
