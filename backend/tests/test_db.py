import pytest

from liberarr.shared import db
from liberarr.shared.config import Settings


def test_migrate_is_idempotent(settings):
    c = db.connect(settings.db_path)
    db.migrate(c)
    db.migrate(c)
    assert c.execute("PRAGMA user_version").fetchone()[0] == len(db.MIGRATIONS)
    tables = {r[0] for r in c.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    assert {"recordings", "transcripts", "summaries", "jobs"} <= tables


@pytest.mark.parametrize(
    "field,value",
    [("secret_key", "change-me"), ("secret_key", "short"), ("api_key", ""), ("admin_password", "change-me"), ("ai_api_key", "")],
)
def test_settings_rejects_placeholder_secrets(tmp_path, field, value):
    kwargs = dict(secret_key="s" * 64, admin_password="pw", api_key="k" * 64, ai_api_key="ai")
    kwargs[field] = value
    with pytest.raises(ValueError) as exc:
        Settings(config_dir=tmp_path, data_dir=tmp_path, **kwargs)
    assert field in str(exc.value)
    if value:
        assert value not in str(exc.value).replace(field, "")
