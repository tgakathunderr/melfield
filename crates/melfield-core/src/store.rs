//! SQLite storage: installs, clones, history, settings. Migrations are
//! append-only and keyed by `PRAGMA user_version`.

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{MelError, MelResult};
use crate::registry::Orb;

const MIGRATIONS: &[&str] = &[
    // v1
    r#"
    CREATE TABLE installs (
        recipe_id   TEXT PRIMARY KEY,
        revision    TEXT NOT NULL,
        dir         TEXT NOT NULL,
        bytes       INTEGER NOT NULL,
        installed_at TEXT NOT NULL
    );
    CREATE TABLE license_accepts (
        recipe_id   TEXT PRIMARY KEY,
        accepted_at TEXT NOT NULL
    );
    CREATE TABLE clones (
        voice_id    TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        recipe_id   TEXT NOT NULL,
        language    TEXT NOT NULL,
        ref_audio   TEXT NOT NULL,
        ref_text    TEXT,
        orb         TEXT NOT NULL,
        consent_at  TEXT NOT NULL,
        consent_text TEXT NOT NULL,
        consent_app_version TEXT NOT NULL,
        watermark   TEXT NOT NULL,
        created_at  TEXT NOT NULL
    );
    CREATE TABLE generations (
        id          TEXT PRIMARY KEY,
        voice_id    TEXT NOT NULL,
        voice_name  TEXT NOT NULL,
        text        TEXT NOT NULL,
        settings    TEXT NOT NULL,
        duration_s  REAL NOT NULL,
        audio_path  TEXT NOT NULL,
        sample_rate INTEGER NOT NULL,
        watermarked INTEGER NOT NULL,
        recipe_id   TEXT NOT NULL,
        favourite   INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL
    );
    CREATE INDEX generations_created ON generations(created_at DESC);
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Install {
    pub recipe_id: String,
    pub revision: String,
    pub dir: String,
    pub bytes: u64,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRecord {
    pub voice_id: String,
    pub name: String,
    pub recipe_id: String,
    pub language: String,
    pub ref_audio: String,
    pub ref_text: Option<String>,
    pub orb: Orb,
    pub consent_at: String,
    pub consent_text: String,
    pub consent_app_version: String,
    pub watermark: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generation {
    pub id: String,
    pub voice_id: String,
    pub voice_name: String,
    pub text: String,
    pub settings: serde_json::Value,
    pub duration_s: f64,
    pub audio_path: String,
    pub sample_rate: u32,
    pub watermarked: bool,
    pub recipe_id: String,
    pub favourite: bool,
    pub created_at: String,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> MelResult<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn in_memory() -> MelResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> MelResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            conn.execute_batch(sql)?;
            conn.pragma_update(None, "user_version", i + 1)?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ---- installs ----
    pub fn put_install(&self, i: &Install) -> MelResult<()> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO installs (recipe_id, revision, dir, bytes, installed_at) VALUES (?1,?2,?3,?4,?5)",
            params![i.recipe_id, i.revision, i.dir, i.bytes as i64, i.installed_at],
        )?;
        Ok(())
    }

    pub fn install(&self, recipe_id: &str) -> MelResult<Option<Install>> {
        Ok(self
            .conn
            .lock()
            .query_row(
                "SELECT recipe_id, revision, dir, bytes, installed_at FROM installs WHERE recipe_id = ?1",
                [recipe_id],
                |r| {
                    Ok(Install {
                        recipe_id: r.get(0)?,
                        revision: r.get(1)?,
                        dir: r.get(2)?,
                        bytes: r.get::<_, i64>(3)? as u64,
                        installed_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn installs(&self) -> MelResult<Vec<Install>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT recipe_id, revision, dir, bytes, installed_at FROM installs")?;
        let rows = stmt.query_map([], |r| {
            Ok(Install {
                recipe_id: r.get(0)?,
                revision: r.get(1)?,
                dir: r.get(2)?,
                bytes: r.get::<_, i64>(3)? as u64,
                installed_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delete_install(&self, recipe_id: &str) -> MelResult<()> {
        self.conn
            .lock()
            .execute("DELETE FROM installs WHERE recipe_id = ?1", [recipe_id])?;
        Ok(())
    }

    // ---- licenses ----
    pub fn accept_license(&self, recipe_id: &str) -> MelResult<()> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO license_accepts (recipe_id, accepted_at) VALUES (?1, ?2)",
            params![recipe_id, now()],
        )?;
        Ok(())
    }

    pub fn license_accepted(&self, recipe_id: &str) -> MelResult<bool> {
        let n: i64 = self.conn.lock().query_row(
            "SELECT COUNT(*) FROM license_accepts WHERE recipe_id = ?1",
            [recipe_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    // ---- clones ----
    pub fn put_clone(&self, c: &CloneRecord) -> MelResult<()> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO clones (voice_id, name, recipe_id, language, ref_audio, ref_text, orb, consent_at, consent_text, consent_app_version, watermark, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                c.voice_id, c.name, c.recipe_id, c.language, c.ref_audio, c.ref_text,
                serde_json::to_string(&c.orb)?, c.consent_at, c.consent_text, c.consent_app_version, c.watermark, c.created_at
            ],
        )?;
        Ok(())
    }

    pub fn clones(&self) -> MelResult<Vec<CloneRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT voice_id, name, recipe_id, language, ref_audio, ref_text, orb, consent_at, consent_text, consent_app_version, watermark, created_at
             FROM clones ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let orb: String = r.get(6)?;
            Ok(CloneRecord {
                voice_id: r.get(0)?,
                name: r.get(1)?,
                recipe_id: r.get(2)?,
                language: r.get(3)?,
                ref_audio: r.get(4)?,
                ref_text: r.get(5)?,
                orb: serde_json::from_str(&orb)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                consent_at: r.get(7)?,
                consent_text: r.get(8)?,
                consent_app_version: r.get(9)?,
                watermark: r.get(10)?,
                created_at: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn clone_record(&self, voice_id: &str) -> MelResult<Option<CloneRecord>> {
        Ok(self.clones()?.into_iter().find(|c| c.voice_id == voice_id))
    }

    pub fn delete_clone(&self, voice_id: &str) -> MelResult<()> {
        self.conn
            .lock()
            .execute("DELETE FROM clones WHERE voice_id = ?1", [voice_id])?;
        Ok(())
    }

    // ---- history ----
    pub fn put_generation(&self, g: &Generation) -> MelResult<()> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO generations (id, voice_id, voice_name, text, settings, duration_s, audio_path, sample_rate, watermarked, recipe_id, favourite, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                g.id, g.voice_id, g.voice_name, g.text, g.settings.to_string(), g.duration_s, g.audio_path,
                g.sample_rate, g.watermarked as i32, g.recipe_id, g.favourite as i32, g.created_at
            ],
        )?;
        Ok(())
    }

    pub fn generations(
        &self,
        query: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> MelResult<Vec<Generation>> {
        let conn = self.conn.lock();
        let pattern = format!(
            "%{}%",
            query.unwrap_or("").replace('%', "\\%").replace('_', "\\_")
        );
        let mut stmt = conn.prepare(
            "SELECT id, voice_id, voice_name, text, settings, duration_s, audio_path, sample_rate, watermarked, recipe_id, favourite, created_at
             FROM generations WHERE text LIKE ?1 ESCAPE '\\' OR voice_name LIKE ?1 ESCAPE '\\'
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(
            params![pattern, limit as i64, offset as i64],
            row_to_generation,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn generation(&self, id: &str) -> MelResult<Option<Generation>> {
        let conn = self.conn.lock();
        Ok(conn
            .query_row(
                "SELECT id, voice_id, voice_name, text, settings, duration_s, audio_path, sample_rate, watermarked, recipe_id, favourite, created_at
                 FROM generations WHERE id = ?1",
                [id],
                row_to_generation,
            )
            .optional()?)
    }

    pub fn set_favourite(&self, id: &str, fav: bool) -> MelResult<()> {
        self.conn.lock().execute(
            "UPDATE generations SET favourite = ?2 WHERE id = ?1",
            params![id, fav as i32],
        )?;
        Ok(())
    }

    pub fn delete_generation(&self, id: &str) -> MelResult<Option<String>> {
        let path = self.generation(id)?.map(|g| g.audio_path);
        self.conn
            .lock()
            .execute("DELETE FROM generations WHERE id = ?1", [id])?;
        Ok(path)
    }

    // ---- settings ----
    pub fn setting<T: serde::de::DeserializeOwned>(&self, key: &str) -> MelResult<Option<T>> {
        let v: Option<String> = self
            .conn
            .lock()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        match v {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> MelResult<()> {
        self.conn.lock().execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
}

fn row_to_generation(r: &rusqlite::Row<'_>) -> rusqlite::Result<Generation> {
    let settings: String = r.get(4)?;
    Ok(Generation {
        id: r.get(0)?,
        voice_id: r.get(1)?,
        voice_name: r.get(2)?,
        text: r.get(3)?,
        settings: serde_json::from_str(&settings).unwrap_or(serde_json::Value::Null),
        duration_s: r.get(5)?,
        audio_path: r.get(6)?,
        sample_rate: r.get(7)?,
        watermarked: r.get::<_, i32>(8)? != 0,
        recipe_id: r.get(9)?,
        favourite: r.get::<_, i32>(10)? != 0,
        created_at: r.get(11)?,
    })
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl From<MelError> for rusqlite::Error {
    fn from(e: MelError) -> Self {
        rusqlite::Error::ToSqlConversionFailure(Box::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen(id: &str, text: &str, at: &str) -> Generation {
        Generation {
            id: id.into(),
            voice_id: "aria".into(),
            voice_name: "Aria".into(),
            text: text.into(),
            settings: serde_json::json!({"speed": 1.0}),
            duration_s: 2.5,
            audio_path: format!("/tmp/{id}.wav"),
            sample_rate: 24000,
            watermarked: false,
            recipe_id: "kokoro-82m".into(),
            favourite: false,
            created_at: at.into(),
        }
    }

    #[test]
    fn history_round_trip_search_and_order() {
        let s = Store::in_memory().unwrap();
        s.put_generation(&gen("a", "hello world", "2026-11-11T10:00:00Z"))
            .unwrap();
        s.put_generation(&gen("b", "goodbye 100%", "2026-11-11T11:00:00Z"))
            .unwrap();
        let all = s.generations(None, 10, 0).unwrap();
        assert_eq!(
            all.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert_eq!(s.generations(Some("hello"), 10, 0).unwrap().len(), 1);
        assert_eq!(s.generations(Some("100%"), 10, 0).unwrap().len(), 1);
        assert_eq!(s.generations(Some("%"), 10, 0).unwrap().len(), 1);
        s.set_favourite("a", true).unwrap();
        assert!(s.generation("a").unwrap().unwrap().favourite);
        assert_eq!(
            s.delete_generation("a").unwrap().as_deref(),
            Some("/tmp/a.wav")
        );
        assert!(s.generation("a").unwrap().is_none());
    }

    #[test]
    fn settings_and_installs() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.setting::<String>("default_voice").unwrap(), None);
        s.set_setting("default_voice", &"aria").unwrap();
        assert_eq!(
            s.setting::<String>("default_voice").unwrap().as_deref(),
            Some("aria")
        );
        s.put_install(&Install {
            recipe_id: "k".into(),
            revision: "r".into(),
            dir: "/d".into(),
            bytes: 5,
            installed_at: now(),
        })
        .unwrap();
        assert_eq!(s.installs().unwrap().len(), 1);
        s.delete_install("k").unwrap();
        assert!(s.install("k").unwrap().is_none());
        assert!(!s.license_accepted("x").unwrap());
        s.accept_license("x").unwrap();
        assert!(s.license_accepted("x").unwrap());
    }

    #[test]
    fn clones_round_trip_with_consent() {
        let s = Store::in_memory().unwrap();
        let c = CloneRecord {
            voice_id: "c1".into(),
            name: "Me".into(),
            recipe_id: "pocket-tts-clone".into(),
            language: "en".into(),
            ref_audio: "/r.wav".into(),
            ref_text: Some("hello".into()),
            orb: Orb {
                c1: [1, 2, 3],
                c2: [4, 5, 6],
                k: 3,
                amp: 0.1,
                ring: false,
            },
            consent_at: now(),
            consent_text: "This is my voice".into(),
            consent_app_version: "0.1.0".into(),
            watermark: "perth".into(),
            created_at: now(),
        };
        s.put_clone(&c).unwrap();
        let got = s.clone_record("c1").unwrap().unwrap();
        assert_eq!(got.consent_text, "This is my voice");
        assert_eq!(got.orb.k, 3);
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.db");
        Store::open(&p).unwrap().set_setting("x", &1).unwrap();
        let s = Store::open(&p).unwrap();
        assert_eq!(s.setting::<i32>("x").unwrap(), Some(1));
    }
}
