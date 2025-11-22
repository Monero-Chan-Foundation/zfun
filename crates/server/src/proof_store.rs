use std::{
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use eyre::Result;
use rusqlite::{Connection, params};

use crate::GetProofResponse;

pub struct ProofStore {
    conn: Mutex<Connection>,
}

impl ProofStore {
    pub fn new(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS proof_requests (
                request_id    TEXT PRIMARY KEY,
                address       BLOB,
                status        TEXT NOT NULL,
                proof_url     TEXT,
                proof_bytes   BLOB,
                public_values BLOB,
                is_tee        BOOLEAN NOT NULL,
                created_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL,
                fulfilled_at  INTEGER,
                amount        INTEGER,
                signature     BLOB UNIQUE NOT NULL,
                signature_timestamp INTEGER,
                had_duplicate_nullifiers BOOLEAN DEFAULT 0
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_proof_requests_signature ON proof_requests(signature);
            CREATE TABLE IF NOT EXISTS used_hashes (
                hash BLOB PRIMARY KEY
            );"#,
        )?;
        
        // Migration: Add had_duplicate_nullifiers column if it doesn't exist
        // This is safe to run multiple times
        conn.execute(
            "ALTER TABLE proof_requests ADD COLUMN had_duplicate_nullifiers BOOLEAN DEFAULT 0",
            [],
        ).or_else(|e| {
            // If column already exists, ignore the error
            if e.to_string().contains("duplicate column name") {
                Ok(0)
            } else {
                Err(e)
            }
        })?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn get_request(&self, request_id: &str) -> rusqlite::Result<Option<GetProofResponse>> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let mut stmt =
            conn.prepare("SELECT status, is_tee, created_at, fulfilled_at, amount, proof_url, COALESCE(had_duplicate_nullifiers, 0) FROM proof_requests WHERE request_id = ?1")?;
        let mut rows = stmt.query(params![request_id])?;
        let row = rows.next()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(GetProofResponse {
            status: row.get(0)?,
            is_tee: row.get(1)?,
            created_at: row.get(2)?,
            fulfilled_at: row.get(3)?,
            balance: row.get(4)?,
            proof_url: row.get(5)?,
            had_duplicate_nullifiers: row.get(6)?,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_request(
        &self,
        request_id: &str,
        status: &str,
        address: &[u8],
        is_tee: bool,
        amount: u64,
        signature: &[u8],
        signature_timestamp: u64,
    ) -> rusqlite::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let signature_timestamp_i64 = signature_timestamp as i64;
        let conn = self.conn.lock().expect("lock proof store connection");
        conn.execute(
            r#"
            INSERT INTO proof_requests (request_id, address, status, proof_url, proof_bytes, public_values, is_tee, created_at, updated_at, amount, signature, signature_timestamp)
            VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?5, ?5, ?6, ?7, ?8)
            ON CONFLICT(request_id) DO UPDATE SET
                address   = excluded.address,
                status    = excluded.status,
                updated_at = excluded.updated_at,
                signature = excluded.signature,
                signature_timestamp = excluded.signature_timestamp
            "#,
            params![request_id, address, status, is_tee, now, amount, signature, signature_timestamp_i64],
        )?;
        Ok(())
    }

    pub fn update_status(
        &self,
        request_id: &str,
        status: &str,
        proof_url: Option<&str>,
        proof_bytes: Option<Vec<u8>>,
        public_values: Option<Vec<u8>>,
        amount: Option<u64>,
    ) -> rusqlite::Result<()> {
        self.update_status_with_duplicates(request_id, status, proof_url, proof_bytes, public_values, amount, false)
    }

    pub fn update_status_with_duplicates(
        &self,
        request_id: &str,
        status: &str,
        proof_url: Option<&str>,
        proof_bytes: Option<Vec<u8>>,
        public_values: Option<Vec<u8>>,
        amount: Option<u64>,
        had_duplicate_nullifiers: bool,
    ) -> rusqlite::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let fulfilled_at = if status == "Fulfilled" {
            Some(now)
        } else {
            None
        };
        let conn = self.conn.lock().expect("lock proof store connection");
        conn.execute(
            "UPDATE proof_requests SET status = ?, proof_url = ?, proof_bytes = ?, public_values = ?, updated_at = ?, amount = coalesce(amount, ?), fulfilled_at = ?, had_duplicate_nullifiers = ? WHERE request_id = ?",
            params![status, proof_url, proof_bytes, public_values, now, amount, fulfilled_at, had_duplicate_nullifiers, request_id],
        )?;
        if conn.changes() == 0 {
            conn.execute(
                "INSERT INTO proof_requests (request_id, address, status, proof_url, proof_bytes, created_at, updated_at, public_values, amount, fulfilled_at, had_duplicate_nullifiers) VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9)",
                params![request_id, status, proof_url, proof_bytes, now, public_values, amount, fulfilled_at, had_duplicate_nullifiers],
            )?;
        }
        Ok(())
    }

    pub fn list_pending_requests(&self) -> rusqlite::Result<Vec<PendingRequest>> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let mut stmt = conn.prepare(
            "SELECT request_id, is_tee FROM proof_requests WHERE status IN ('Processing', 'Assigned')",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PendingRequest {
                request_id: row.get(0)?,
                is_tee: row.get(1)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn record_used_hashes(&self, hashes: &[&[u8]]) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let mut stmt = conn.prepare("INSERT INTO used_hashes (hash) VALUES (?1)")?;
        for hash in hashes {
            stmt.execute(params![hash])?;
        }
        Ok(())
    }

    /// Record used hashes with demo mode support
    /// In demo mode, allows duplicate hashes and returns whether any were already used
    pub fn record_used_hashes_demo(&self, hashes: &[&[u8]], demo_mode: bool) -> rusqlite::Result<(bool, Vec<Vec<u8>>)> {
        let conn = self.conn.lock().expect("lock proof store connection");

        if demo_mode {
            // Check which hashes already exist
            let mut already_used = Vec::new();
            let mut check_stmt = conn.prepare("SELECT hash FROM used_hashes WHERE hash = ?1")?;
            for hash in hashes {
                let mut result = check_stmt.query(params![hash])?;
                if result.next()?.is_some() {
                    already_used.push(hash.to_vec());
                }
            }

            let had_duplicates = !already_used.is_empty();

            // Insert with OR IGNORE to allow duplicates in demo mode
            let mut stmt = conn.prepare("INSERT OR IGNORE INTO used_hashes (hash) VALUES (?1)")?;
            for hash in hashes {
                stmt.execute(params![hash])?;
            }

            Ok((had_duplicates, already_used))
        } else {
            // Production mode - strict duplicate prevention
            let mut stmt = conn.prepare("INSERT INTO used_hashes (hash) VALUES (?1)")?;
            for hash in hashes {
                stmt.execute(params![hash])?;
            }
            Ok((false, Vec::new()))
        }
    }

    pub fn get_used_hashes(&self, hashes: Vec<Vec<u8>>) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let mut stmt = conn.prepare("SELECT hash FROM used_hashes WHERE hash = ?1")?;
        for hash in hashes {
            let mut result = stmt.query(params![hash])?;
            if result.next()?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn get_hourly_proof_counts(&self, hours: i64) -> rusqlite::Result<Vec<HourlyProofCount>> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now - (hours * 3600);

        // Group by hour, counting proofs created in each hour
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                (created_at / 3600) * 3600 as hour_timestamp,
                COUNT(*) as count
            FROM proof_requests
            WHERE created_at >= ?1
            GROUP BY hour_timestamp
            ORDER BY hour_timestamp ASC
            "#,
        )?;
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok(HourlyProofCount {
                hour_timestamp: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn get_total_before_timestamp(&self, timestamp: i64) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM proof_requests WHERE created_at < ?1",
                params![timestamp],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(total)
    }

    pub fn get_hourly_amounts(&self, hours: i64) -> rusqlite::Result<Vec<(i64, i64)>> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now - (hours * 3600);

        let mut stmt = conn.prepare(
            r#"
            SELECT 
                (created_at / 3600) * 3600 as hour_timestamp,
                SUM(COALESCE(amount, 0)) as total_amount
            FROM proof_requests
            WHERE created_at >= ?1 AND amount IS NOT NULL
            GROUP BY hour_timestamp
            ORDER BY hour_timestamp ASC
            "#,
        )?;

        let rows = stmt.query_map(params![cutoff], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn get_total_amount_before_timestamp(&self, timestamp: i64) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let total: i64 = conn.query_row(
            "SELECT SUM(COALESCE(amount, 0)) FROM proof_requests WHERE created_at < ?1 AND amount IS NOT NULL",
            params![timestamp],
            |row| Ok(row.get::<_, Option<i64>>(0)?.unwrap_or(0)),
        )?;
        Ok(total)
    }

    pub fn signature_exists(&self, signature: &[u8]) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let mut stmt = conn.prepare("SELECT 1 FROM proof_requests WHERE signature = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![signature])?;
        Ok(rows.next()?.is_some())
    }

    pub fn get_recent_proofs_with_addresses(
        &self,
        limit: i64,
    ) -> rusqlite::Result<Vec<(RecentProof, Option<Vec<u8>>)>> {
        let conn = self.conn.lock().expect("lock proof store connection");
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                request_id,
                created_at,
                is_tee,
                amount,
                address
            FROM proof_requests
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                RecentProof {
                    request_id: row.get(0)?,
                    created_at: row.get(1)?,
                    is_tee: row.get(2)?,
                    amount: row.get(3)?,
                },
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

#[derive(Debug)]
pub struct HourlyProofCount {
    pub hour_timestamp: i64,
    pub count: i64,
}

#[derive(Debug)]
pub struct RecentProof {
    pub request_id: String,
    pub created_at: i64,
    #[allow(dead_code)]
    pub is_tee: bool,
    pub amount: Option<i64>,
}

pub struct PendingRequest {
    pub request_id: String,
    pub is_tee: bool,
}
