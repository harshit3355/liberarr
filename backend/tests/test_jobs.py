import pytest

from liberarr import jobs, worker


@pytest.fixture
def rid(conn):
    return conn.execute(
        "INSERT INTO recordings (source, source_ref, audio_path) VALUES ('upload', 'ref', '/x') RETURNING id"
    ).fetchone()[0]


def statuses(conn, rid):
    return [(r["step"], r["status"]) for r in conn.execute("SELECT step, status FROM jobs WHERE recording_id=? ORDER BY id", (rid,))]


def test_complete_chains_next_step(conn, rid):
    jobs.enqueue(conn, rid, "transcribe")
    job = jobs.claim_next(conn)
    assert job.step == "transcribe" and job.attempts == 1
    jobs.complete(conn, job.id)
    assert statuses(conn, rid) == [("transcribe", "done"), ("summarize", "queued")]
    jobs.complete(conn, jobs.claim_next(conn).id)
    jobs.complete(conn, jobs.claim_next(conn).id)
    assert statuses(conn, rid)[-1] == ("export", "done")
    assert jobs.claim_next(conn) is None


def test_fail_records_error_and_stops_chain(conn, rid):
    jobs.enqueue(conn, rid, "transcribe")
    job = jobs.claim_next(conn)
    jobs.fail(conn, job.id, "boom" * 1000)
    row = conn.execute("SELECT status, error FROM jobs WHERE id=?", (job.id,)).fetchone()
    assert row["status"] == "failed" and len(row["error"]) == 2000
    assert jobs.claim_next(conn) is None


def test_reset_stale_requeues_running(conn, rid):
    jobs.enqueue(conn, rid, "transcribe")
    jobs.claim_next(conn)
    assert jobs.reset_stale(conn) == 1
    assert statuses(conn, rid) == [("transcribe", "queued")]


def test_rerun_unknown_step_raises(conn, rid):
    with pytest.raises(ValueError):
        jobs.rerun(conn, rid, "nope")


def test_rerun_enqueues_step(conn, rid):
    jobs.rerun(conn, rid, "summarize")
    assert statuses(conn, rid) == [("summarize", "queued")]


def test_run_once_marks_failed_on_exception(conn, rid):
    jobs.enqueue(conn, rid, "transcribe")

    def boom(c, r):
        raise RuntimeError("provider down")

    assert worker.run_once(conn, {"transcribe": boom}) is True
    row = conn.execute("SELECT status, error FROM jobs").fetchone()
    assert row["status"] == "failed" and "provider down" in row["error"]


def test_run_once_success_completes(conn, rid):
    jobs.enqueue(conn, rid, "transcribe")
    seen = []
    assert worker.run_once(conn, {"transcribe": lambda c, r: seen.append(r)}) is True
    assert seen == [rid]
    assert statuses(conn, rid) == [("transcribe", "done"), ("summarize", "queued")]


def test_run_once_empty_returns_false(conn):
    assert worker.run_once(conn, {}) is False
