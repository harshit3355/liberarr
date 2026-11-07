//! SQLite adapters: one `Db` implements every storage port of `liberarr-core`.
mod jobs;
mod repos;
mod schema;

use liberarr_core::{Error, Result};
use rusqlite::types::Type;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Storage errors never carry row content: SQLite messages name tables and columns, not values.
fn storage(e: impl std::fmt::Display) -> Error {
    Error::Storage(e.to_string())
}

/// Timestamps are stored as RFC3339 UTC text.
fn now() -> String {
    jiff::Timestamp::now().to_string()
}

fn parse_ts(col: usize, s: &str) -> rusqlite::Result<jiff::Timestamp> {
    s.parse()
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(col, Type::Text, Box::new(e)))
}

/// One SQLite connection behind a mutex; every port call runs on the blocking pool.
/// Cheap to clone: clones share the connection.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (creating if needed) the database: WAL, foreign keys, 5 s busy timeout, migrations.
    pub fn open(path: &Path) -> Result<Db> {
        let mut conn = Connection::open(path).map_err(storage)?;
        // Before any other statement, so concurrent openers wait instead of failing.
        conn.busy_timeout(Duration::from_millis(5000))
            .map_err(storage)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(storage)?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(storage)?;
        schema::migrate(&mut conn).map_err(storage)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` on the blocking pool with the connection.
    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            // A panic in an earlier call must not wedge the DB: SQLite state is transactional.
            let mut guard = conn.lock().unwrap_or_else(PoisonError::into_inner);
            f(&mut guard)
        })
        .await
        .map_err(storage)?
        .map_err(storage)
    }

    #[cfg(test)]
    fn user_version(&self) -> Result<i64> {
        let c = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        c.pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage)
    }
}

/// Write transactions take the lock up front (`BEGIN IMMEDIATE`) so they never fail on upgrade.
fn write_tx(conn: &mut Connection) -> rusqlite::Result<Transaction<'_>> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use liberarr_core::device::ports::DeviceLedger;
    use liberarr_core::jobs::{JobQueue, JobStatus};
    use liberarr_core::recordings::{NewRecording, RecordingRepo, RecordingStatus, Source};
    use std::path::PathBuf;

    /// A fresh DB file under the OS temp dir, removed on drop (with its WAL side files).
    struct TempDb(PathBuf);
    impl TempDb {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "liberarr-sqlite-{tag}-{}-{}",
                std::process::id(),
                jiff::Timestamp::now().as_nanosecond()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> PathBuf {
            self.0.join("liberarr.db")
        }
    }
    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_rec(source_ref: &str) -> NewRecording {
        NewRecording {
            title: Some("t".into()),
            source: Source::File,
            source_ref: source_ref.into(),
            audio_path: "/audio/a.ogg".into(),
            duration_s: Some(1.5),
            created_at: "2026-03-01T10:00:00Z".parse().unwrap(),
        }
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let t = TempDb::new("migrate");
        let db = Db::open(&t.path()).unwrap();
        let id = db.add(new_rec("h1")).await.unwrap();
        drop(db);
        // Re-opening must neither fail nor touch existing data.
        let db = Db::open(&t.path()).unwrap();
        assert_eq!(db.find_by_ref("h1").await.unwrap(), Some(id));
        assert_eq!(db.user_version().unwrap(), schema::MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn claim_is_exclusive_across_connections() {
        let t = TempDb::new("claim");
        let (a, b) = (Db::open(&t.path()).unwrap(), Db::open(&t.path()).unwrap());
        let rec = a.add(new_rec("h1")).await.unwrap();
        a.enqueue(rec, "transcribe").await.unwrap();

        let (r1, r2, r3, r4) = tokio::join!(
            a.claim_next(),
            b.claim_next(),
            a.claim_next(),
            b.claim_next()
        );
        let claims = [r1, r2, r3, r4]
            .into_iter()
            .filter_map(|r| r.unwrap())
            .count();
        assert_eq!(claims, 1);
    }

    #[tokio::test]
    async fn reset_stale_requeues_running() {
        let t = TempDb::new("stale");
        let db = Db::open(&t.path()).unwrap();
        let rec = db.add(new_rec("h1")).await.unwrap();
        db.enqueue(rec, "transcribe").await.unwrap();
        let job = db.claim_next().await.unwrap().unwrap();
        assert_eq!(job.attempts, 1);
        assert_eq!(
            db.list_items().await.unwrap()[0].status,
            RecordingStatus::Processing
        );

        assert_eq!(db.reset_stale().await.unwrap(), 1);
        assert_eq!(db.list_for(rec).await.unwrap()[0].status, JobStatus::Queued);

        // Claimed again, then failed: the read model reports the failing step.
        let job = db.claim_next().await.unwrap().unwrap();
        assert_eq!(job.attempts, 2);
        db.fail(&job, "boom").await.unwrap();
        let item = &db.list_items().await.unwrap()[0];
        assert_eq!(item.status, RecordingStatus::Failed);
        assert_eq!(item.failed_step.as_deref(), Some("transcribe"));
    }

    #[tokio::test]
    async fn ledger_roundtrip() {
        let t = TempDb::new("ledger");
        let db = Db::open(&t.path()).unwrap();
        let rec = db.add(new_rec("h1")).await.unwrap();
        assert!(db.known("dev-a").await.unwrap().is_empty());

        db.record("dev-a", 7, 1000, rec).await.unwrap();
        db.record("dev-a", 7, 1000, rec).await.unwrap(); // idempotent
        db.record("dev-a", 9, 2000, rec).await.unwrap();

        assert_eq!(db.known("dev-a").await.unwrap(), [7, 9].into());
        assert!(db.known("dev-b").await.unwrap().is_empty());
    }
}
