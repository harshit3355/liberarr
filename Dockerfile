# All base images pinned by tag + digest (AGENTS.md: no floating tags).
FROM node:22.21.1-trixie-slim@sha256:c3bf4cf764467f1bf9789fde549971a2cf8e720196df6cf3f95bafa590e5f4af AS ui
WORKDIR /ui
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM ghcr.io/astral-sh/uv:0.13.3@sha256:bee0a2a458fd956c5df7b8b4c5fed2257b0af34866e3da7b51db0be3acb375d1 AS uv

FROM python:3.13.9-slim-trixie@sha256:326df678c20c78d465db501563f3492d17c42a4afe33a1f2bf5406a1d56b0e86
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg && rm -rf /var/lib/apt/lists/*
COPY --from=uv /uv /usr/local/bin/uv
ENV UV_PYTHON_DOWNLOADS=never UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy \
    UV_PROJECT_ENVIRONMENT=/app/.venv PATH=/app/.venv/bin:$PATH
WORKDIR /app
COPY backend/pyproject.toml backend/uv.lock backend/.python-version ./
RUN uv sync --frozen --no-dev --no-install-project
COPY backend/src ./src
RUN uv sync --frozen --no-dev
COPY --from=ui /ui/dist /app/static
ENV LIBERARR_STATIC_DIR=/app/static
RUN useradd --uid 1000 --create-home liberarr && mkdir -p /config /data && chown liberarr /config /data
USER liberarr
EXPOSE 8484
CMD ["uvicorn", "--factory", "liberarr.api.app:build_app", "--host", "0.0.0.0", "--port", "8484"]
