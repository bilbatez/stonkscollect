//! Thin [`libsql`] (Turso) wrapper giving sqlx-like ergonomics: `query`/`execute`
//! with positional `?` params and **named** column access, so the rest of the
//! store reads naturally and the SQLite-compatible SQL strings are unchanged.
//!
//! A single [`Db`] is config-driven: a remote Turso URL in production, or a
//! local libSQL file (`:memory:` in tests) otherwise. libSQL speaks SQLite SQL.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use libsql::{Builder, Connection, Value};

/// Errors from the database layer.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("libsql error: {0}")]
    Lib(#[from] libsql::Error),
    #[error("decode error: {0}")]
    Decode(String),
}

type Result<T> = std::result::Result<T, DbError>;

/// A bound parameter. `From` impls cover every type the store binds.
#[derive(Clone)]
pub enum P {
    Int(i64),
    Real(f64),
    Text(String),
    Null,
}

impl From<P> for Value {
    fn from(p: P) -> Value {
        match p {
            P::Int(i) => Value::Integer(i),
            P::Real(r) => Value::Real(r),
            P::Text(s) => Value::Text(s),
            P::Null => Value::Null,
        }
    }
}

impl From<i64> for P {
    fn from(v: i64) -> P {
        P::Int(v)
    }
}
impl From<i32> for P {
    fn from(v: i32) -> P {
        P::Int(v as i64)
    }
}
impl From<f64> for P {
    fn from(v: f64) -> P {
        P::Real(v)
    }
}
impl From<bool> for P {
    fn from(v: bool) -> P {
        P::Int(v as i64)
    }
}
impl From<String> for P {
    fn from(v: String) -> P {
        P::Text(v)
    }
}
impl From<&str> for P {
    fn from(v: &str) -> P {
        P::Text(v.to_string())
    }
}
impl From<&String> for P {
    fn from(v: &String) -> P {
        P::Text(v.clone())
    }
}
impl From<NaiveDate> for P {
    fn from(v: NaiveDate) -> P {
        P::Text(v.format("%Y-%m-%d").to_string())
    }
}
impl From<DateTime<Utc>> for P {
    fn from(v: DateTime<Utc>) -> P {
        P::Text(v.to_rfc3339())
    }
}
impl<T: Into<P>> From<Option<T>> for P {
    fn from(v: Option<T>) -> P {
        match v {
            Some(x) => x.into(),
            None => P::Null,
        }
    }
}
impl<T: Into<P> + Clone> From<&Option<T>> for P {
    fn from(v: &Option<T>) -> P {
        match v {
            Some(x) => x.clone().into(),
            None => P::Null,
        }
    }
}

/// Build a positional parameter vector: `params![a, b, &c]`.
#[macro_export]
macro_rules! params {
    () => { Vec::<$crate::store::db::P>::new() };
    ($($x:expr),+ $(,)?) => { vec![$($crate::store::db::P::from($x)),+] };
}

fn to_values(params: Vec<P>) -> Vec<Value> {
    params.into_iter().map(Value::from).collect()
}

/// Decode a [`Value`] into a concrete Rust type.
pub trait FromVal: Sized {
    fn from_val(v: &Value) -> Result<Self>;
}

fn type_err(want: &str, v: &Value) -> DbError {
    DbError::Decode(format!("expected {want}, got {v:?}"))
}

impl FromVal for i64 {
    fn from_val(v: &Value) -> Result<i64> {
        match v {
            Value::Integer(i) => Ok(*i),
            _ => Err(type_err("integer", v)),
        }
    }
}
impl FromVal for f64 {
    fn from_val(v: &Value) -> Result<f64> {
        match v {
            Value::Real(r) => Ok(*r),
            Value::Integer(i) => Ok(*i as f64),
            _ => Err(type_err("real", v)),
        }
    }
}
impl FromVal for bool {
    fn from_val(v: &Value) -> Result<bool> {
        Ok(i64::from_val(v)? != 0)
    }
}
impl FromVal for String {
    fn from_val(v: &Value) -> Result<String> {
        match v {
            Value::Text(s) => Ok(s.clone()),
            _ => Err(type_err("text", v)),
        }
    }
}
impl FromVal for NaiveDate {
    fn from_val(v: &Value) -> Result<NaiveDate> {
        let s = String::from_val(v)?;
        NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| DbError::Decode(format!("date {s}: {e}")))
    }
}
impl FromVal for DateTime<Utc> {
    fn from_val(v: &Value) -> Result<DateTime<Utc>> {
        let s = String::from_val(v)?;
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| DbError::Decode(format!("datetime {s}: {e}")))
    }
}
impl<T: FromVal> FromVal for Option<T> {
    fn from_val(v: &Value) -> Result<Option<T>> {
        match v {
            Value::Null => Ok(None),
            _ => Ok(Some(T::from_val(v)?)),
        }
    }
}

/// One fetched row with name → value access (sqlx `try_get` style).
pub struct Row {
    names: Arc<Vec<String>>,
    vals: Vec<Value>,
}

impl Row {
    /// Decode the named column. Errors if the column is absent or mistyped.
    pub fn get<T: FromVal>(&self, col: &str) -> Result<T> {
        let idx = self
            .names
            .iter()
            .position(|n| n == col)
            .ok_or_else(|| DbError::Decode(format!("no such column: {col}")))?;
        T::from_val(&self.vals[idx])
    }

    /// sqlx-style alias for [`Row::get`].
    pub fn try_get<T: FromVal>(&self, col: &str) -> Result<T> {
        self.get(col)
    }
}

/// A config-driven libSQL handle. Connects per call so concurrent collection
/// (parallel `buffer_unordered`) never shares one connection.
#[derive(Clone)]
pub struct Db {
    /// One long-lived connection shared by every call. libSQL opens a *new*
    /// connection per `Database::connect()`, and on Linux a fresh connection does
    /// not see another connection's committed WAL write (nor does `:memory:`,
    /// which is per-connection) — so connect-per-call crashed in the container
    /// with "no such table". Sharing one connection keeps every read/write on the
    /// same view; libSQL serializes concurrent statements internally. The
    /// `Connection` holds its own ref to the database, so no separate handle is kept.
    conn: Connection,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Db {
    /// Open a local libSQL file (`:memory:` for tests). Enables WAL so the
    /// read-only API runs concurrently with the collection loop's writers
    /// (libSQL local defaults to a rollback journal, which blocks readers).
    pub async fn open_local(path: &str) -> Result<Db> {
        let db = Builder::new_local(path).build().await?;
        let conn = db.connect()?;
        // journal_mode returns the resulting mode as a row, so use query (execute
        // rejects row-returning statements with ExecuteReturnedRows). WAL is set
        // once on the shared connection.
        let _ = conn.query("PRAGMA journal_mode=WAL", ()).await?;
        Ok(Db { conn, closed: Arc::new(false.into()) })
    }

    /// Open a remote Turso database.
    pub async fn open_remote(url: String, token: String) -> Result<Db> {
        let db = Builder::new_remote(url, token).build().await?;
        let conn = db.connect()?;
        Ok(Db { conn, closed: Arc::new(false.into()) })
    }

    /// Mark the handle closed so subsequent queries error (exercises best-effort
    /// failure paths in tests).
    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The shared connection. Every call returns the *same* underlying connection
    /// (cheap clone — libSQL `Connection` is an `Arc` internally) so writes are
    /// visible to later reads regardless of platform.
    fn connect(&self) -> Result<Connection> {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(DbError::Decode("database is closed".into()));
        }
        Ok(self.conn.clone())
    }

    /// A connection for write paths: foreign keys enforced + a busy timeout so
    /// concurrent collection writers wait for the lock instead of failing with
    /// SQLITE_BUSY and silently dropping the write.
    async fn write_conn(&self) -> Result<Connection> {
        let c = self.connect()?;
        c.execute("PRAGMA foreign_keys=ON", ()).await?;
        // busy_timeout returns the new value as a row, so use query, not execute.
        let _ = c.query("PRAGMA busy_timeout=5000", ()).await?;
        Ok(c)
    }

    /// Run multi-statement SQL (used by the migration runner).
    pub async fn batch(&self, sql: &str) -> Result<()> {
        self.connect()?.execute_batch(sql).await?;
        Ok(())
    }

    /// Execute a write, returning affected rows. Foreign keys are enforced.
    pub async fn execute(&self, sql: &str, params: Vec<P>) -> Result<u64> {
        Ok(self.write_conn().await?.execute(sql, to_values(params)).await?)
    }

    async fn rows(&self, sql: &str, params: Vec<P>) -> Result<Vec<Row>> {
        let conn = self.connect()?;
        let mut rows = conn.query(sql, to_values(params)).await?;
        let n = rows.column_count();
        let names: Arc<Vec<String>> = Arc::new(
            (0..n).map(|i| rows.column_name(i).unwrap_or("").to_string()).collect(),
        );
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            let vals = (0..n).map(|i| r.get_value(i)).collect::<std::result::Result<Vec<_>, _>>()?;
            out.push(Row { names: names.clone(), vals });
        }
        Ok(out)
    }

    /// All matching rows.
    pub async fn query(&self, sql: &str, params: Vec<P>) -> Result<Vec<Row>> {
        self.rows(sql, params).await
    }

    /// The first row, if any.
    pub async fn query_opt(&self, sql: &str, params: Vec<P>) -> Result<Option<Row>> {
        Ok(self.rows(sql, params).await?.into_iter().next())
    }

    /// The first column of the first row, if any.
    pub async fn scalar_opt<T: FromVal>(&self, sql: &str, params: Vec<P>) -> Result<Option<T>> {
        match self.query_opt(sql, params).await? {
            Some(r) => Ok(Some(T::from_val(&r.vals[0])?)),
            None => Ok(None),
        }
    }

    /// The first column of the first row (errors if none).
    pub async fn scalar<T: FromVal>(&self, sql: &str, params: Vec<P>) -> Result<T> {
        self.scalar_opt(sql, params)
            .await?
            .ok_or_else(|| DbError::Decode("query returned no rows".into()))
    }

    /// Begin a transaction (foreign keys enforced).
    pub async fn begin(&self) -> Result<Tx> {
        let conn = self.write_conn().await?;
        let tx = conn.transaction().await?;
        Ok(Tx { tx })
    }
}

/// An open transaction. Statements run on its connection; `commit` finalizes.
pub struct Tx {
    tx: libsql::Transaction,
}

impl Tx {
    /// Execute a write within the transaction.
    pub async fn execute(&self, sql: &str, params: Vec<P>) -> Result<u64> {
        Ok(self.tx.execute(sql, to_values(params)).await?)
    }

    /// Commit the transaction.
    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await?;
        Ok(())
    }
}

// --- sqlx-shaped fluent layer (so store call sites read like the old code) ---

/// Something a write can run against: the [`Db`] or an open [`Tx`].
#[async_trait::async_trait]
pub trait Exec {
    async fn exec(&self, sql: &str, params: Vec<P>) -> Result<u64>;
}

#[async_trait::async_trait]
impl Exec for Db {
    async fn exec(&self, sql: &str, params: Vec<P>) -> Result<u64> {
        self.execute(sql, params).await
    }
}

#[async_trait::async_trait]
impl Exec for Tx {
    async fn exec(&self, sql: &str, params: Vec<P>) -> Result<u64> {
        self.execute(sql, params).await
    }
}

/// A row query builder (`query(sql).bind(a).fetch_all(&db)`).
pub struct Query {
    sql: String,
    params: Vec<P>,
}

/// Start a row query.
pub fn query(sql: &str) -> Query {
    Query { sql: sql.to_string(), params: Vec::new() }
}

impl Query {
    pub fn bind(mut self, v: impl Into<P>) -> Self {
        self.params.push(v.into());
        self
    }
    pub async fn fetch_all(self, db: &Db) -> Result<Vec<Row>> {
        db.query(&self.sql, self.params).await
    }
    pub async fn fetch_optional(self, db: &Db) -> Result<Option<Row>> {
        db.query_opt(&self.sql, self.params).await
    }
    pub async fn execute(self, ex: &impl Exec) -> Result<u64> {
        ex.exec(&self.sql, self.params).await
    }
}

/// A scalar query builder (`query_scalar::<i64>(sql).bind(a).fetch_one(&db)`).
pub struct Scalar<T> {
    sql: String,
    params: Vec<P>,
    _t: std::marker::PhantomData<T>,
}

/// Start a scalar query (first column of the first row).
pub fn query_scalar<T: FromVal>(sql: &str) -> Scalar<T> {
    Scalar { sql: sql.to_string(), params: Vec::new(), _t: std::marker::PhantomData }
}

impl<T: FromVal> Scalar<T> {
    pub fn bind(mut self, v: impl Into<P>) -> Self {
        self.params.push(v.into());
        self
    }
    pub async fn fetch_one(self, db: &Db) -> Result<T> {
        db.scalar(&self.sql, self.params).await
    }
    pub async fn fetch_optional(self, db: &Db) -> Result<Option<T>> {
        db.scalar_opt(&self.sql, self.params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn file_db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        (Db::open_local(path.to_str().unwrap()).await.unwrap(), dir)
    }

    #[tokio::test]
    async fn conversions_queries_tx_and_close() {
        let (db, _dir) = file_db().await;
        db.batch("CREATE TABLE t (i INTEGER, r REAL, s TEXT, b INTEGER, d TEXT, ts TEXT, n TEXT)")
            .await
            .unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
        let ts = chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let owned = String::from("hi");
        let opt_s: Option<String> = Some("o".into());
        // every P conversion: i64, i32, f64, bool, &str, &String, NaiveDate, DateTime, Option(None), &Option(Some)
        db.execute(
            "INSERT INTO t (i,r,s,b,d,ts,n) VALUES (?,?,?,?,?,?,?)",
            params![7i64, 1.5f64, "hi", true, date, ts, Option::<i64>::None],
        )
        .await
        .unwrap();
        db.execute("INSERT INTO t (i,s,n) VALUES (?,?,?)", params![3i32, &owned, &opt_s])
            .await
            .unwrap();

        let rows = db.query("SELECT i,r,s,b,d,ts,n FROM t ORDER BY i", params![]).await.unwrap();
        let r = &rows[1]; // i=7
        assert_eq!(r.get::<i64>("i").unwrap(), 7);
        assert_eq!(r.get::<f64>("r").unwrap(), 1.5);
        assert_eq!(r.try_get::<String>("s").unwrap(), "hi");
        assert!(r.get::<bool>("b").unwrap());
        assert_eq!(r.get::<chrono::NaiveDate>("d").unwrap(), date);
        assert_eq!(r.get::<chrono::DateTime<chrono::Utc>>("ts").unwrap(), ts);
        assert!(r.get::<Option<i64>>("n").unwrap().is_none());
        assert_eq!(rows[0].get::<f64>("i").unwrap(), 3.0); // f64 from INTEGER
        // error paths: missing column, type mismatch, bad date/datetime
        assert!(r.get::<i64>("nope").is_err());
        assert!(r.get::<String>("i").is_err());
        assert!(rows[0].get::<chrono::NaiveDate>("s").is_err());
        assert!(rows[0].get::<chrono::DateTime<chrono::Utc>>("s").is_err());

        // scalar / scalar_opt / query_opt
        assert_eq!(db.scalar::<i64>("SELECT COUNT(*) FROM t", params![]).await.unwrap(), 2);
        assert!(db.scalar_opt::<i64>("SELECT i FROM t WHERE i=999", params![]).await.unwrap().is_none());
        assert!(db.query_opt("SELECT i FROM t WHERE i=999", params![]).await.unwrap().is_none());
        assert!(db.scalar::<i64>("SELECT i FROM t WHERE i=999", params![]).await.is_err());

        // fluent builders + Tx (Exec for Tx and Db)
        let tx = db.begin().await.unwrap();
        query("INSERT INTO t (i) VALUES (?)").bind(100i64).execute(&tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            query_scalar::<i64>("SELECT i FROM t WHERE i=?").bind(100i64).fetch_one(&db).await.unwrap(),
            100
        );
        assert!(query_scalar::<i64>("SELECT i FROM t WHERE i=?")
            .bind(7i64)
            .fetch_optional(&db)
            .await
            .unwrap()
            .is_some());
        assert!(query("SELECT i FROM t WHERE i=?").bind(7i64).fetch_optional(&db).await.unwrap().is_some());
        assert!(!query("SELECT i FROM t").bind(0i64).fetch_all(&db).await.unwrap().is_empty());

        // close -> all paths error
        db.close();
        assert!(db.query("SELECT 1", params![]).await.is_err());
        assert!(db.execute("SELECT 1", params![]).await.is_err());
        assert!(db.begin().await.is_err());
    }

    #[tokio::test]
    async fn operations_share_one_connection() {
        // A write through one Db call must be visible to a later call. With the old
        // connect-per-call shim this failed on `:memory:` (a fresh DB per connect)
        // and on Linux WAL (a new connection couldn't see the prior connection's
        // committed write) — the exact "no such table: _migrations" container crash.
        let db = Db::open_local(":memory:").await.unwrap();
        db.batch("CREATE TABLE t (i INTEGER)").await.unwrap();
        db.execute("INSERT INTO t (i) VALUES (?)", params![42i64]).await.unwrap();
        assert_eq!(db.scalar::<i64>("SELECT i FROM t", params![]).await.unwrap(), 42);
    }

    #[tokio::test]
    async fn concurrent_transactions_on_shared_connection() {
        // Bulk collect persists many companies at once (buffer_unordered), each via
        // its own begin()/commit() on the shared connection. Assert that N
        // concurrent transactions all land and none error/clobber.
        let (db, _dir) = file_db().await;
        db.batch("CREATE TABLE t (i INTEGER PRIMARY KEY)").await.unwrap();
        let db = std::sync::Arc::new(db);
        let tasks = (0..16i64).map(|i| {
            let db = db.clone();
            async move {
                let tx = db.begin().await?;
                query("INSERT INTO t (i) VALUES (?)").bind(i).execute(&tx).await?;
                tx.commit().await
            }
        });
        let results = futures::future::join_all(tasks).await;
        let ok = results.iter().filter(|r| r.is_ok()).count();
        let n = db.scalar::<i64>("SELECT COUNT(*) FROM t", params![]).await.unwrap();
        assert_eq!(ok, 16, "all transactions should commit");
        assert_eq!(n, 16, "all rows should land");
    }

    #[tokio::test]
    async fn open_remote_builds_a_handle() {
        // build() constructs the client without connecting; covers the ctor.
        let _ = Db::open_remote("libsql://example.invalid".into(), "tok".into()).await;
    }

    #[test]
    fn dberror_display() {
        assert!(DbError::Decode("x".into()).to_string().contains("decode"));
    }
}
