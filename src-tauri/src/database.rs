use std::{fs, path::Path, sync::Mutex};

use anyhow::{bail, Context, Result};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};

use crate::protocol::{
    AnswerPayload, AskPayload, QueueSummary, RequestContext, RequestOrigin, RequestStatus,
    StoredRequest,
};

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "busy_timeout", 5_000_i64)?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS requests (
              sequence INTEGER PRIMARY KEY AUTOINCREMENT,
              request_id TEXT NOT NULL UNIQUE,
              payload_json TEXT NOT NULL,
              payload_hash TEXT NOT NULL,
              status TEXT NOT NULL CHECK(status IN ('pending', 'answered', 'canceled')),
              result_json TEXT,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              completed_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS requests_status_sequence
              ON requests(status, sequence);
            ",
        )?;
        let version =
            connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
        if version < 2 {
            connection.execute_batch(
                "
                ALTER TABLE requests ADD COLUMN origin_json TEXT;
                ALTER TABLE requests ADD COLUMN context_json TEXT;
                PRAGMA user_version = 2;
                ",
            )?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn insert_or_get(
        &self,
        request_id: &str,
        payload: &AskPayload,
        origin: Option<&RequestOrigin>,
        context: Option<&RequestContext>,
    ) -> Result<StoredRequest> {
        payload.validate()?;
        if let Some(origin) = origin {
            origin.validate()?;
        }
        if let Some(context) = context {
            context.validate()?;
        }
        let payload_json = serde_json::to_string(payload)?;
        let payload_hash = payload.hash()?;
        let origin_json = origin.map(serde_json::to_string).transpose()?;
        let context_json = context.map(serde_json::to_string).transpose()?;
        let now = Utc::now().timestamp_millis();
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let existing = transaction
            .query_row(
                "SELECT sequence, request_id, payload_json, payload_hash, origin_json, context_json,
                        status, result_json, created_at, updated_at, completed_at
                   FROM requests WHERE request_id = ?1",
                [request_id],
                row_to_request_with_hash,
            )
            .optional()?;
        if let Some((request, existing_hash)) = existing {
            if existing_hash != payload_hash {
                bail!("request ID is already associated with a different payload");
            }
            transaction.commit()?;
            return Ok(request);
        }

        transaction.execute(
            "INSERT INTO requests
              (request_id, payload_json, payload_hash, origin_json, context_json, status,
               created_at, updated_at)
              VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?6)",
            params![
                request_id,
                payload_json,
                payload_hash,
                origin_json,
                context_json,
                now
            ],
        )?;
        let request = transaction.query_row(
            "SELECT sequence, request_id, payload_json, origin_json, context_json, status,
                    result_json, created_at, updated_at, completed_at
               FROM requests WHERE request_id = ?1",
            [request_id],
            row_to_request,
        )?;
        transaction.commit()?;
        Ok(request)
    }

    pub fn get(&self, request_id: &str) -> Result<Option<StoredRequest>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(
                "SELECT sequence, request_id, payload_json, origin_json, context_json, status,
                        result_json, created_at, updated_at, completed_at
                   FROM requests WHERE request_id = ?1",
                [request_id],
                row_to_request,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn active(&self) -> Result<Option<StoredRequest>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(
                "SELECT sequence, request_id, payload_json, origin_json, context_json, status,
                        result_json, created_at, updated_at, completed_at
                   FROM requests WHERE status = 'pending' ORDER BY sequence LIMIT 1",
                [],
                row_to_request,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn summary(&self) -> Result<QueueSummary> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let pending = connection.query_row(
            "SELECT COUNT(*) FROM requests WHERE status = 'pending'",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        let active_request_id = connection
            .query_row(
                "SELECT request_id FROM requests
                 WHERE status = 'pending' ORDER BY sequence LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(QueueSummary {
            pending,
            active_request_id,
        })
    }

    pub fn answer(&self, request_id: &str, result: &AnswerPayload) -> Result<StoredRequest> {
        let current = self
            .get(request_id)?
            .with_context(|| format!("request {request_id} was not found"))?;
        if current.status != RequestStatus::Pending {
            bail!("request is no longer pending");
        }
        current.payload.validate_answer(result)?;

        let now = Utc::now().timestamp_millis();
        let result_json = serde_json::to_string(result)?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        let changed = connection.execute(
            "UPDATE requests SET status = 'answered', result_json = ?2,
                    updated_at = ?3, completed_at = ?3
              WHERE request_id = ?1 AND status = 'pending'",
            params![request_id, result_json, now],
        )?;
        if changed != 1 {
            bail!("request is no longer pending");
        }
        drop(connection);
        self.get(request_id)?
            .context("answered request disappeared")
    }

    pub fn cancel(&self, request_id: &str) -> Result<StoredRequest> {
        let now = Utc::now().timestamp_millis();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "UPDATE requests SET status = 'canceled', updated_at = ?2, completed_at = ?2
              WHERE request_id = ?1 AND status = 'pending'",
            params![request_id, now],
        )?;
        drop(connection);
        self.get(request_id)?
            .with_context(|| format!("request {request_id} was not found"))
    }

    pub fn cleanup_completed(&self) -> Result<usize> {
        let cutoff = (Utc::now() - Duration::hours(24)).timestamp_millis();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .execute(
                "DELETE FROM requests
                 WHERE status != 'pending' AND completed_at IS NOT NULL AND completed_at < ?1",
                [cutoff],
            )
            .map_err(Into::into)
    }
}

fn row_to_request(row: &Row<'_>) -> rusqlite::Result<StoredRequest> {
    let payload_json: String = row.get(2)?;
    let origin_json: Option<String> = row.get(3)?;
    let context_json: Option<String> = row.get(4)?;
    let status: String = row.get(5)?;
    let result_json: Option<String> = row.get(6)?;
    Ok(StoredRequest {
        sequence: row.get(0)?,
        request_id: row.get(1)?,
        payload: serde_json::from_str(&payload_json).map_err(json_error)?,
        origin: origin_json
            .map(|value| serde_json::from_str(&value).map_err(json_error))
            .transpose()?,
        context: context_json
            .map(|value| serde_json::from_str(&value).map_err(json_error))
            .transpose()?,
        status: RequestStatus::parse(&status).map_err(other_error)?,
        result: result_json
            .map(|value| serde_json::from_str(&value).map_err(json_error))
            .transpose()?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}

fn row_to_request_with_hash(row: &Row<'_>) -> rusqlite::Result<(StoredRequest, String)> {
    let payload_json: String = row.get(2)?;
    let payload_hash: String = row.get(3)?;
    let origin_json: Option<String> = row.get(4)?;
    let context_json: Option<String> = row.get(5)?;
    let status: String = row.get(6)?;
    let result_json: Option<String> = row.get(7)?;
    Ok((
        StoredRequest {
            sequence: row.get(0)?,
            request_id: row.get(1)?,
            payload: serde_json::from_str(&payload_json).map_err(json_error)?,
            origin: origin_json
                .map(|value| serde_json::from_str(&value).map_err(json_error))
                .transpose()?,
            context: context_json
                .map(|value| serde_json::from_str(&value).map_err(json_error))
                .transpose()?,
            status: RequestStatus::parse(&status).map_err(other_error)?,
            result: result_json
                .map(|value| serde_json::from_str(&value).map_err(json_error))
                .transpose()?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            completed_at: row.get(10)?,
        },
        payload_hash,
    ))
}

fn json_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn other_error(error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::protocol::{AnswerValue, Question, QuestionOption};

    fn payload(question: &str) -> AskPayload {
        AskPayload {
            questions: vec![Question {
                question: question.into(),
                header: "Choice".into(),
                options: vec![
                    QuestionOption {
                        label: "A".into(),
                        description: "First".into(),
                        preview: None,
                    },
                    QuestionOption {
                        label: "B".into(),
                        description: "Second".into(),
                        preview: None,
                    },
                ],
                multi_select: false,
            }],
        }
    }

    #[test]
    fn persists_fifo_and_idempotency() {
        let directory = std::env::temp_dir().join(format!("auq-db-{}", uuid::Uuid::now_v7()));
        let database = Database::open(&directory.join("queue.sqlite3")).unwrap();
        let origin = RequestOrigin {
            agent: "codex".into(),
            cwd: Some("/Projects/auq-wizard".into()),
            project_root: Some("/Projects/auq-wizard".into()),
            project_name: Some("auq-wizard".into()),
            branch: Some("main".into()),
            session_id: None,
        };
        let context = RequestContext {
            summary: "Choose the first option.".into(),
        };
        database
            .insert_or_get("one", &payload("First?"), Some(&origin), Some(&context))
            .unwrap();
        database
            .insert_or_get("two", &payload("Second?"), None, None)
            .unwrap();
        assert_eq!(database.active().unwrap().unwrap().request_id, "one");
        let first = database.get("one").unwrap().unwrap();
        assert_eq!(first.origin, Some(origin));
        assert_eq!(first.context, Some(context));
        assert!(database
            .insert_or_get("one", &payload("Different?"), None, None)
            .unwrap_err()
            .to_string()
            .contains("different payload"));

        database
            .answer(
                "one",
                &AnswerPayload {
                    answers: Some(BTreeMap::from([(
                        "First?".into(),
                        AnswerValue::Single("A".into()),
                    )])),
                    response: None,
                },
            )
            .unwrap();
        assert_eq!(database.active().unwrap().unwrap().request_id, "two");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn migrates_legacy_requests_to_origin_context_columns() {
        let directory = std::env::temp_dir().join(format!("auq-db-{}", uuid::Uuid::now_v7()));
        let path = directory.join("queue.sqlite3");
        fs::create_dir_all(&directory).unwrap();
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "
                CREATE TABLE requests (
                  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                  request_id TEXT NOT NULL UNIQUE,
                  payload_json TEXT NOT NULL,
                  payload_hash TEXT NOT NULL,
                  status TEXT NOT NULL CHECK(status IN ('pending', 'answered', 'canceled')),
                  result_json TEXT,
                  created_at INTEGER NOT NULL,
                  updated_at INTEGER NOT NULL,
                  completed_at INTEGER
                );
                CREATE INDEX requests_status_sequence ON requests(status, sequence);
                PRAGMA user_version = 1;
                ",
            )
            .unwrap();
        drop(legacy);

        let database = Database::open(&path).unwrap();
        let origin = RequestOrigin {
            agent: "codex".into(),
            cwd: None,
            project_root: None,
            project_name: Some("auq-wizard".into()),
            branch: None,
            session_id: None,
        };
        database
            .insert_or_get("migrated", &payload("Choose?"), Some(&origin), None)
            .unwrap();
        assert_eq!(
            database.get("migrated").unwrap().unwrap().origin,
            Some(origin)
        );
        let _ = fs::remove_dir_all(directory);
    }
}
