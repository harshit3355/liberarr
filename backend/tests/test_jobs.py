import pytest

from liberarr.jobs.application import JobRunner, rerun
from liberarr.jobs.domain import UnknownStep
from liberarr.jobs.infrastructure import SqliteJobQueue


@pytest.fixture
def q(conn):
    return SqliteJobQueue(conn)


@pytest.fixture
def rid(conn):
    return conn.execute(
        "INSERT INTO recordings (source, source_ref, audio_path) VALUES ('upload', 'ref', '/x') RETURNING id"
    ).fetchone()[0]


def statuses(conn, rid):
    return [(r["step"], r["status"]) for r in conn.execute("SELECT step, status FROM jobs WHERE recording_id=? ORDER BY id", (rid,))]


def test_complete_chains_next_step(conn, q, rid):
    q.enqueue(rid, "transcribe")
    job = q.claim_next()
    assert job.step == "transcribe" and job.attempts == 1
    q.complete(job, then="summarize")
    assert statuses(conn, rid) == [("transcribe", "done"), ("summarize", "queued")]
    JobRunner(q, {"summarize": lambda r: None, "export": lambda r: None}).run_once()
    JobRunner(q, {"summarize": lambda r: None, "export": lambda r: None}).run_once()
    assert statuses(conn, rid)[-1] == ("export", "done")
    assert q.claim_next() is None


def test_fail_records_error_and_stops_chain(conn, q, rid):
    q.enqueue(rid, "transcribe")
    job = q.claim_next()
    q.fail(job, "boom" * 1000)
    row = conn.execute("SELECT status, error FROM jobs WHERE id=?", (job.id,)).fetchone()
    assert row["status"] == "failed" and len(row["error"]) == 2000
    assert q.claim_next() is None


def test_reset_stale_requeues_running(conn, q, rid):
    q.enqueue(rid, "transcribe")
    q.claim_next()
    assert q.reset_stale() == 1
    assert statuses(conn, rid) == [("transcribe", "queued")]


def test_rerun_unknown_step_raises(conn, q, rid):
    with pytest.raises(UnknownStep):
        rerun(q, rid, "nope")


def test_rerun_enqueues_step(conn, q, rid):
    rerun(q, rid, "summarize")
    assert statuses(conn, rid) == [("summarize", "queued")]


def test_run_once_marks_failed_on_exception(conn, q, rid):
    q.enqueue(rid, "transcribe")

    def boom(r):
        raise RuntimeError("provider down")

    assert JobRunner(q, {"transcribe": boom}).run_once() is True
    row = conn.execute("SELECT status, error FROM jobs").fetchone()
    assert row["status"] == "failed" and "provider down" in row["error"]


def test_run_once_success_completes(conn, q, rid):
    q.enqueue(rid, "transcribe")
    seen = []
    assert JobRunner(q, {"transcribe": seen.append}).run_once() is True
    assert seen == [rid]
    assert statuses(conn, rid) == [("transcribe", "done"), ("summarize", "queued")]


def test_run_once_empty_returns_false(q):
    assert JobRunner(q, {}).run_once() is False
