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
