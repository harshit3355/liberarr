# Liberarr

Self-hosted, *arr-style companion for voice recorders (Plaud Note & co.): drop an audio file, get a transcript, a meeting summary, a mind map and an action table — in a web UI and as an Obsidian note — with the AI provider of your choice.

## Quick start

Requirements: Docker with Compose.

```sh
./scripts/init-env.sh        # creates .env, generates internal secrets, prints the admin password once
$EDITOR .env                 # add the provider keys used in litellm.yaml (e.g. OPENAI_API_KEY, ANTHROPIC_API_KEY)
docker compose up -d --build
```

Open <http://localhost:8484> and log in as `admin`.

Add recordings by uploading them in the UI, or by dropping files into `data/inbox/`. The folder is checked every 30 s, and a file is picked up once its size stops changing.

## Secrets

All secrets live in **one file: `.env`**. It is gitignored, created by `scripts/init-env.sh`, and has `chmod 600`.

- No other file holds a secret value. `litellm.yaml` only references `os.environ/…`, and the compose file passes `LITELLM_MASTER_KEY` to the app by interpolation.
- The app refuses to start if a secret is missing, still `change-me`, or too short.
- Run `./scripts/check-no-secrets.sh` before committing. It is a good pre-commit hook.

## Choosing AI providers

`litellm.yaml` maps two aliases:

- `transcribe`: speech-to-text, OpenAI Whisper by default (Groq is commented out).
- `summarize`: summary, mind map and actions, Claude by default.

To switch provider, edit `litellm_params` and run `docker compose restart litellm`. Liberarr only talks to the OpenAI-compatible LiteLLM gateway.

> ⚠️ Every recording goes to the providers configured in `litellm.yaml`, including work meetings. Per-type routing and a local transcription engine come in later releases.

## Data and moving machines

| Path | Content |
|---|---|
| `config/` | SQLite database |
| `data/audio/` | original recordings, untouched |
| `data/inbox/` | drop folder (`rejected/` for non-audio files) |
| `data/obsidian/` | one Markdown note per recording |

To move to another machine (e.g. Mac → NAS), copy `config/`, `data/` **and** `.env`, then run `docker compose up -d --build`.

The container runs as UID 1000. On a NAS, make sure `config/` and `data/` are writable by that UID.

## Obsidian

Point the folder at your vault, either with `LIBERARR_OBSIDIAN_DIR` or by mounting `data/obsidian` inside your vault. The export is one-way:

- Liberarr only rewrites what lies between `%% liberarr:start %%` and `%% liberarr:end %%`.
- Anything you add outside those markers is kept.
- Ticking a task in Obsidian does not sync back to Liberarr.

## Development

```sh
cd backend && uv run pytest -q                   # backend tests (needs ffmpeg)
cd frontend && npm ci && npm run dev             # UI on :5173, proxies /api to :8484
```

Rules for contributors and AI agents: see [AGENTS.md](AGENTS.md).
