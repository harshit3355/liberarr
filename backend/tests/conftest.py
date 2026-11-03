import subprocess
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from liberarr import db
from liberarr.api.app import create_app
from liberarr.config import Settings


@pytest.fixture
def settings(tmp_path: Path) -> Settings:
    s = Settings(
        config_dir=tmp_path / "config",
        data_dir=tmp_path / "data",
        secret_key="s" * 64,
        admin_password="admin-password",
        api_key="k" * 64,
        ai_api_key="ai-key",
        inbox_poll_seconds=0,
    )
    for d in (s.config_dir, s.data_dir, s.audio_dir, s.inbox_dir):
        d.mkdir(parents=True, exist_ok=True)
    return s


@pytest.fixture
def conn(settings: Settings):
    c = db.connect(settings.db_path)
    db.migrate(c)
    yield c
    c.close()


@pytest.fixture
def client(settings: Settings, conn) -> TestClient:
    return TestClient(create_app(settings))


@pytest.fixture
def audio_file(tmp_path: Path):
    counter = iter(range(10_000))

    def make(seconds: float, freq: int = 440) -> Path:
        out = tmp_path / f"tone-{next(counter)}.mp3"
        subprocess.run(
            ["ffmpeg", "-v", "error", "-f", "lavfi", "-i", f"sine=frequency={freq}:duration={seconds}", str(out)],
            check=True,
        )
        return out

    return make
