from functools import lru_cache
from pathlib import Path

from pydantic import model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict

PLACEHOLDER = "change-me"


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="LIBERARR_")

    config_dir: Path = Path("/config")
    data_dir: Path = Path("/data")
    secret_key: str = ""
    admin_username: str = "admin"
    admin_password: str = ""
    api_key: str = ""
    ai_base_url: str = "http://litellm:4000"
    ai_api_key: str = ""
    transcribe_model: str = "transcribe"
    llm_model: str = "summarize"
    obsidian_dir: Path | None = None
    cookie_secure: bool = False
    inbox_poll_seconds: int = 30
    static_dir: Path | None = None

    @model_validator(mode="after")
    def _check_secrets(self) -> "Settings":
        # Name the variable, never its value.
        for name in ("secret_key", "admin_password", "api_key", "ai_api_key"):
            value = getattr(self, name)
            if not value or value == PLACEHOLDER:
                raise ValueError(f"LIBERARR_{name.upper()} ({name}) is empty or still a placeholder; run scripts/init-env.sh")
        for name in ("secret_key", "api_key"):
            if len(getattr(self, name)) < 32:
                raise ValueError(f"LIBERARR_{name.upper()} ({name}) must be at least 32 characters")
        return self

    @property
    def db_path(self) -> Path:
        return self.config_dir / "liberarr.db"

    @property
    def audio_dir(self) -> Path:
        return self.data_dir / "audio"

    @property
    def inbox_dir(self) -> Path:
        return self.data_dir / "inbox"

    @property
    def obsidian_path(self) -> Path:
        return self.obsidian_dir or self.data_dir / "obsidian"


@lru_cache
def get_settings() -> Settings:
    return Settings()
