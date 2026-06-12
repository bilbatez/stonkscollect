use super::*;
use sqlx::Row;

impl Store {
    /// Record the start of a collection run, returning its id.
    pub async fn start_run(
        &self,
        source: &str,
        scope: Option<&str>,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO collection_runs (source,scope,started_at,status) VALUES (?,?,?,'running') RETURNING id",
        )
        .bind(source)
        .bind(scope)
        .bind(started_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Mark a collection run finished with a final status and optional error.
    pub async fn finish_run(
        &self,
        id: i64,
        status: &str,
        finished_at: chrono::DateTime<chrono::Utc>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE collection_runs SET status=?, finished_at=?, error=? WHERE id=?",
        )
        .bind(status)
        .bind(finished_at)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Record per-source failures for one company's ingest pass.
    pub async fn save_source_errors(
        &self,
        company_id: i64,
        errors: &[(String, String)],
        occurred_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        if errors.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for (source, message) in errors {
            sqlx::query(
                "INSERT INTO source_errors (company_id,source,message,occurred_at) VALUES (?,?,?,?)",
            )
            .bind(company_id)
            .bind(source)
            .bind(message)
            .bind(occurred_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// A company's most recent source failures, newest first.
    pub async fn recent_source_errors(
        &self,
        company_id: i64,
        limit: i64,
    ) -> Result<Vec<SourceError>> {
        let rows = sqlx::query(
            "SELECT source,message,occurred_at FROM source_errors \
             WHERE company_id=? ORDER BY occurred_at DESC, id DESC LIMIT ?",
        )
        .bind(company_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(SourceError {
                    source: r.try_get("source")?,
                    message: r.try_get("message")?,
                    occurred_at: r.try_get("occurred_at")?,
                })
            })
            .collect()
    }

    /// Stored HTTP cache validators for `url`, if any.
    pub async fn http_validators(&self, url: &str) -> Result<Option<Validators>> {
        let row = sqlx::query("SELECT etag,last_modified FROM http_cache WHERE url=?")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| {
            Ok(Validators {
                etag: r.try_get("etag")?,
                last_modified: r.try_get("last_modified")?,
            })
        })
        .transpose()
    }

    /// Upsert the cache validators a fresh response for `url` arrived with.
    pub async fn save_http_validators(&self, url: &str, v: &Validators) -> Result<()> {
        sqlx::query(
            "INSERT INTO http_cache (url,etag,last_modified) VALUES (?,?,?) \
             ON CONFLICT(url) DO UPDATE SET etag=excluded.etag, \
             last_modified=excluded.last_modified, fetched_at=datetime('now')",
        )
        .bind(url)
        .bind(&v.etag)
        .bind(&v.last_modified)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List the most recent collection runs, newest first.
    pub async fn recent_runs(&self, limit: i64) -> Result<Vec<CollectionRun>> {
        let rows = sqlx::query(
            "SELECT id,source,scope,started_at,finished_at,status,error FROM collection_runs \
             ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(CollectionRun {
                    id: r.try_get("id")?,
                    source: r.try_get("source")?,
                    scope: r.try_get("scope")?,
                    started_at: r.try_get("started_at")?,
                    finished_at: r.try_get("finished_at")?,
                    status: r.try_get("status")?,
                    error: r.try_get("error")?,
                })
            })
            .collect()
    }
}

/// Best-effort adapter: cache failures degrade to unconditional fetches
/// instead of failing the collection.
#[async_trait::async_trait(?Send)]
impl crate::net::HttpValidatorRepo for Store {
    async fn validators(&self, url: &str) -> Option<Validators> {
        match self.http_validators(url).await {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!("http_cache read failed for {url}: {e}");
                None
            }
        }
    }

    async fn store_validators(&self, url: &str, validators: &Validators) {
        if let Err(e) = self.save_http_validators(url, validators).await {
            tracing::debug!("http_cache write failed for {url}: {e}");
        }
    }
}
