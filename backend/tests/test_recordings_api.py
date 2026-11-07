import pytest


@pytest.fixture
def authed(client, settings):
    client.headers["X-Api-Key"] = settings.api_key
    return client


def upload(client, path):
    with path.open("rb") as f:
        return client.post("/api/recordings", files={"file": (path.name, f, "audio/mpeg")})


def test_recordings_require_auth(client):
    assert client.get("/api/recordings").status_code == 401


def test_upload_201_then_duplicate_200(authed, audio_file):
    src = audio_file(1)
    r1 = upload(authed, src)
    assert r1.status_code == 201
    r2 = upload(authed, src)
    assert r2.status_code == 200 and r2.json() == r1.json()


def test_upload_non_audio_400(authed, tmp_path):
    bad = tmp_path / "x.mp3"
    bad.write_text("nope")
    r = upload(authed, bad)
    assert r.status_code == 400 and r.json() == {"detail": "not an audio file"}


def test_list_status_reflects_jobs(authed, conn, audio_file):
    rid = upload(authed, audio_file(1)).json()["id"]
    [item] = authed.get("/api/recordings").json()
    assert item["id"] == rid and item["status"] == "processing" and item["failed_step"] is None
    conn.execute("UPDATE jobs SET status='failed', error='x'")
    [item] = authed.get("/api/recordings").json()
    assert item["status"] == "failed" and item["failed_step"] == "transcribe"
    conn.execute("UPDATE jobs SET status='done'")
    assert authed.get("/api/recordings").json()[0]["status"] == "done"


def test_get_recording_detail(authed, audio_file):
    rid = upload(authed, audio_file(1)).json()["id"]
    body = authed.get(f"/api/recordings/{rid}").json()
    assert body["id"] == rid and body["transcript"] is None and body["summary"] is None
    assert [j["step"] for j in body["jobs"]] == ["transcribe"]
    audio = authed.get(f"/api/recordings/{rid}/audio", headers={"Range": "bytes=0-9"})
    assert audio.status_code == 206 and len(audio.content) == 10


def test_get_recording_404(authed):
    assert authed.get("/api/recordings/999").status_code == 404


def test_rerun_unknown_step_400(authed, audio_file):
    rid = upload(authed, audio_file(1)).json()["id"]
    assert authed.post(f"/api/recordings/{rid}/rerun", json={"step": "nope"}).status_code == 400
    assert authed.post(f"/api/recordings/{rid}/rerun", json={"step": "transcribe"}).status_code == 202
