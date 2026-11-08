# First run

The usual path: `init` once, `link` once, then `pull` whenever you have new recordings, and `tui` or `serve` to read the results.

## 1. `liberarr init`: create the configuration

```sh
liberarr init
```

Creates `~/.liberarr` (or `LIBERARR_HOME`) and its `.env` file (mode 0600). It:

- generates the secrets (`LIBERARR_SECRET_KEY`, `LIBERARR_API_KEY`) and the web UI admin password;
- asks for the transcription provider, then the summary provider: URL, API key (hidden input), model. Each key is tested and the account's models are proposed. See [AI providers](ai-providers.md) to choose;
- asks for the language of summaries and notes (`en` or `fr`).

At the end it prints the web UI password **once** (user `admin`). It stays in `.env` as `LIBERARR_ADMIN_PASSWORD`.

Running `init` again is safe: values already set are kept. When everything is set, it offers to keep the configuration or to reconfigure the providers.

Options:

- `--ui tui`: the same questions, full-screen. `LIBERARR_UI=tui` in the shell does the same.
- `--non-interactive`: asks nothing. Export `LIBERARR_TRANSCRIBE_PROVIDER`, `_BASE_URL`, `_API_KEY`, `_MODEL` and the same four `LIBERARR_LLM_*` variables in the shell first (keys never go on the command line). The base URL and the transcription model can be left out when the provider has a default. Fails, naming the variable, when one is missing.

## 2. `liberarr link`: unlock your device (once)

Liberarr needs the device keys, which only your Plaud account has. `link` fetches them once:

1. Log in at [web.plaud.ai](https://web.plaud.ai) in a browser.
2. Open the browser DevTools, then Application (Storage in Firefox) → Cookies, and copy the value of `pld_ut`.
3. Wake the device (press a button) and close the Plaud app on your phone.
4. Run `liberarr link` and paste the token (input hidden).

The token is used for this run only and never stored. The keys are stored encrypted in `~/.liberarr/device/`. Then `link` looks for the device (about 12 s) and tests an unlock.

A key pair already stored is kept; `liberarr link --force` fetches a new one.

## 3. `liberarr pull`: get the recordings

```sh
liberarr pull
```

Wake the device and close the Plaud app on your phone first: the device talks to one Bluetooth client at a time. `pull` finds the device (about 12 s), downloads new recordings with progress, then processes them: transcription, summary, export of a note.

- A dropped download resumes on the next `pull`.
- A recording already pulled is never pulled again.
- A recording still in progress on the device is skipped.
- `--no-process`: only download. The work waits in the queue and runs in `serve`, `tui`, or the next `pull` or `process`.

## 4. Read the results: `liberarr tui` or `liberarr serve`

**Terminal app** (`liberarr tui`): the list of recordings and their status; open one to see its summary, timestamped transcript, mind map, actions to tick and jobs. Keys are shown at the bottom of each screen (`p` pull, `r` rerun, `s` settings, `/` search, `q` quit). Without a configuration it starts with the setup. It follows `NO_COLOR` (no colors, no animation) and `LIBERARR_REDUCED_MOTION=1` (no animation).

**Web UI** (`liberarr serve`): open <http://127.0.0.1:8484> and log in as `admin` with the password from `init`. `--host` and `--port` change the address; see [Privacy](privacy.md#web-ui-on-the-network) before using `--host 0.0.0.0`. `serve` never talks to the device: run `pull` separately.

Both run the job worker, and both can run at the same time.

Each processed recording is also written as a Markdown note in `~/.liberarr/obsidian/` (or `LIBERARR_OBSIDIAN_DIR`, see [Configuration](configuration.md)).

## Other commands

| Command | What it does |
|---|---|
| `liberarr process <files…>` | Imports local audio files and runs the pipeline on them. A file already imported is not duplicated. Ogg files of any size work; other formats must stay under about 24 MB (the providers' upload limit). Prints the number (`#id`) of each recording. |
| `liberarr rerun <id> [--step transcribe\|summarize\|export]` | Queues a step again, and the steps after it. Without `--step`: the failed step (an error if none failed). Refused while the recording is being processed. Prior outputs are kept until the step succeeds. The step runs in `serve` or `tui`, or with the next `pull` or `process`. |
| `liberarr status` | Known devices, last pull, number of recordings in progress, failed or without speech. |

A recording with no speech (a pocket recording, silence) gets a transcript but no summary and no note.

Next: [Configuration](configuration.md).
