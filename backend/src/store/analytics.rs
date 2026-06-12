use super::*;
use sqlx::Row;

/// Screener filters + paging. `Default` matches everything, first page.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenFilter {
    pub defensive_only: bool,
    pub net_net_only: bool,
    pub min_score: i64,
    pub sector: Option<String>,
    pub min_pe: Option<f64>,
    pub max_pe: Option<f64>,
    pub min_roe: Option<f64>,
    pub max_de: Option<f64>,
    pub min_margin: Option<f64>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for ScreenFilter {
    fn default() -> Self {
        Self {
            defensive_only: false,
            net_net_only: false,
            min_score: 0,
            sector: None,
            min_pe: None,
            max_pe: None,
            min_roe: None,
            max_de: None,
            min_margin: None,
            sort_by: None,
            sort_dir: None,
            limit: 50,
            offset: 0,
        }
    }
}

impl Store {
    // --- Graham scores / screener ---

    /// The most recent close price for a company.
    pub async fn latest_price(&self, company_id: i64) -> Result<Option<f64>> {
        Ok(
            sqlx::query_scalar("SELECT close FROM prices WHERE company_id=? ORDER BY date DESC LIMIT 1")
                .bind(company_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Insert or update a company's Graham score.
    pub async fn save_graham_score(&self, s: &GrahamScore) -> Result<()> {
        sqlx::query(
            "INSERT INTO graham_scores \
             (company_id,score,passes_defensive,graham_number,ncav_per_share,margin_of_safety,net_net,computed_at) \
             VALUES (?,?,?,?,?,?,?,?) \
             ON CONFLICT(company_id) DO UPDATE SET \
             score=excluded.score, passes_defensive=excluded.passes_defensive, \
             graham_number=excluded.graham_number, ncav_per_share=excluded.ncav_per_share, \
             margin_of_safety=excluded.margin_of_safety, net_net=excluded.net_net, computed_at=excluded.computed_at",
        )
        .bind(s.company_id)
        .bind(s.score)
        .bind(s.passes_defensive)
        .bind(s.graham_number)
        .bind(s.ncav_per_share)
        .bind(s.margin_of_safety)
        .bind(s.net_net)
        .bind(s.computed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn graham_score_from_row(r: &sqlx::sqlite::SqliteRow) -> Result<GrahamScore> {
        Ok(GrahamScore {
            company_id: r.try_get("company_id")?,
            score: r.try_get("score")?,
            passes_defensive: r.try_get("passes_defensive")?,
            graham_number: r.try_get("graham_number")?,
            ncav_per_share: r.try_get("ncav_per_share")?,
            margin_of_safety: r.try_get("margin_of_safety")?,
            net_net: r.try_get("net_net")?,
            computed_at: r.try_get("computed_at")?,
        })
    }

    /// A company's persisted Graham score, if computed.
    pub async fn get_graham_score(&self, company_id: i64) -> Result<Option<GrahamScore>> {
        let row = sqlx::query(
            "SELECT company_id,score,passes_defensive,graham_number,ncav_per_share,margin_of_safety,net_net,computed_at \
             FROM graham_scores WHERE company_id=?",
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(Self::graham_score_from_row(&r)?)),
            None => Ok(None),
        }
    }

    /// Screen companies by Graham score per `filter`. Returns the page plus the
    /// total number of matches (for pagination).
    pub async fn screen(&self, filter: &ScreenFilter) -> Result<(Vec<(Company, GrahamScore)>, i64)> {
        let mut ratio_q = ScreenQueryBuilder::new();
        ratio_q.add_ratio_filter("pe", filter.min_pe, filter.max_pe);
        ratio_q.add_ratio_filter("roe", filter.min_roe, None);
        ratio_q.add_ratio_filter("debt_to_equity", None, filter.max_de);
        ratio_q.add_ratio_filter("net_margin", filter.min_margin, None);

        let base_joins = " FROM graham_scores g JOIN companies c ON c.id = g.company_id";
        let mut where_clause = String::from(" WHERE g.score >= ?");
        if filter.defensive_only {
            where_clause.push_str(" AND g.passes_defensive = 1");
        }
        if filter.net_net_only {
            where_clause.push_str(" AND g.net_net = 1");
        }
        if filter.sector.is_some() {
            where_clause.push_str(" AND c.sector = ?");
        }
        let from_clause = format!(
            "{base_joins}{}{where_clause}{}",
            ratio_q.extra_joins, ratio_q.extra_conditions
        );
        let count_sql = format!("SELECT COUNT(*){from_clause}");
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(filter.min_score);
        if let Some(s) = &filter.sector {
            count_q = count_q.bind(s);
        }
        for v in &ratio_q.binds {
            count_q = count_q.bind(*v);
        }
        let total = count_q.fetch_one(&self.pool).await?;
        let order_by = screen_sort_expr(filter.sort_by.as_deref(), filter.sort_dir.as_deref());
        let sql = format!(
            "SELECT {SELECT_COMPANY_COLS}, {SELECT_GRAHAM_COLS}{from_clause} ORDER BY {order_by} LIMIT ? OFFSET ?"
        );
        let mut query = sqlx::query(&sql).bind(filter.min_score);
        if let Some(s) = &filter.sector {
            query = query.bind(s);
        }
        for v in &ratio_q.binds {
            query = query.bind(*v);
        }
        let rows = query
            .bind(filter.limit)
            .bind(filter.offset)
            .fetch_all(&self.pool)
            .await?;
        let page = rows
            .iter()
            .map(|r| Ok((company_from_row(r)?, Self::graham_score_from_row(r)?)))
            .collect::<Result<Vec<_>>>()?;
        Ok((page, total))
    }

    /// Sector-level aggregates for the overview page, ordered by avg_score desc.
    /// Companies with no sector are excluded.
    pub async fn get_sectors(&self) -> Result<Vec<crate::domain::SectorStats>> {
        let rows = sqlx::query(
            "SELECT c.sector, \
             COUNT(DISTINCT c.id) AS company_count, \
             COALESCE(AVG(g.score), 0.0) AS avg_score, \
             COALESCE(AVG(CASE WHEN g.passes_defensive = 1 THEN 1.0 ELSE 0.0 END), 0.0) AS pct_defensive, \
             (SELECT c2.ticker FROM companies c2 \
              LEFT JOIN graham_scores g2 ON g2.company_id = c2.id \
              WHERE c2.sector = c.sector \
              ORDER BY COALESCE(g2.score, -1) DESC, c2.ticker ASC LIMIT 1) AS top_ticker \
             FROM companies c \
             LEFT JOIN graham_scores g ON g.company_id = c.id \
             WHERE c.sector IS NOT NULL \
             GROUP BY c.sector \
             ORDER BY avg_score DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(crate::domain::SectorStats {
                    sector: r.try_get("sector")?,
                    company_count: r.try_get("company_count")?,
                    avg_score: r.try_get("avg_score")?,
                    pct_defensive: r.try_get("pct_defensive")?,
                    top_ticker: r.try_get("top_ticker")?,
                })
            })
            .collect()
    }

    /// Same-sector companies (excluding `company_id`) sorted by Graham score desc.
    pub async fn get_peers(
        &self,
        company_id: i64,
        sector: Option<&str>,
        limit: i64,
    ) -> Result<Vec<(Company, Option<GrahamScore>)>> {
        let Some(sector) = sector else {
            return Ok(vec![]);
        };
        let sql = format!(
            "SELECT {SELECT_COMPANY_COLS}, {SELECT_GRAHAM_COLS} \
             FROM companies c LEFT JOIN graham_scores g ON g.company_id = c.id \
             WHERE c.sector = ? AND c.id != ? \
             ORDER BY COALESCE(g.score, -1) DESC, c.ticker ASC LIMIT ?"
        );
        let rows = sqlx::query(&sql)
        .bind(sector)
        .bind(company_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let score: Option<i64> = r.try_get("score")?;
                let g = match score {
                    Some(_) => Some(Self::graham_score_from_row(r)?),
                    None => None,
                };
                Ok((company_from_row(r)?, g))
            })
            .collect()
    }

    /// Retrieve a user's note for a company, if any.
    pub async fn get_note(&self, user_id: i64, company_id: i64) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT body FROM notes WHERE user_id = ? AND company_id = ?")
                .bind(user_id)
                .bind(company_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Insert or update a user's note for a company.
    pub async fn save_note(
        &self,
        user_id: i64,
        company_id: i64,
        body: &str,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO notes (user_id, company_id, body, updated_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT (user_id, company_id) DO UPDATE SET body = excluded.body, updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(company_id)
        .bind(body)
        .bind(updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a user's note for a company (no-op if absent).
    pub async fn delete_note(&self, user_id: i64, company_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM notes WHERE user_id = ? AND company_id = ?")
            .bind(user_id)
            .bind(company_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A page of companies with each one's Graham score when computed, plus the
    /// total count. `q` filters by ticker/name substring. `sort_by`/`sort_dir`
    /// control ordering (whitelisted — no injection risk).
    pub async fn list_companies(
        &self,
        q: Option<&str>,
        sort_by: Option<&str>,
        sort_dir: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<(Company, Option<GrahamScore>)>, i64)> {
        let like = q.map(|s| {
            let e = s.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            format!("%{}%", e)
        });
        let where_clause = if like.is_some() {
            " WHERE c.ticker LIKE ? ESCAPE '\\' OR c.name LIKE ? ESCAPE '\\'"
        } else {
            ""
        };
        let count_sql = format!("SELECT COUNT(*) FROM companies c{where_clause}");
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(l) = &like {
            count_q = count_q.bind(l.as_str()).bind(l.as_str());
        }
        let total = count_q.fetch_one(&self.pool).await?;

        let order_by = companies_sort_expr(sort_by, sort_dir);
        let sql = format!(
            "SELECT {SELECT_COMPANY_COLS}, {SELECT_GRAHAM_COLS} \
             FROM companies c LEFT JOIN graham_scores g ON g.company_id = c.id{where_clause} \
             ORDER BY {order_by} LIMIT ? OFFSET ?"
        );
        let mut query = sqlx::query(&sql);
        if let Some(l) = &like {
            query = query.bind(l.as_str()).bind(l.as_str());
        }
        let rows = query.bind(limit).bind(offset).fetch_all(&self.pool).await?;
        let page = rows
            .iter()
            .map(|r| {
                let score: Option<i64> = r.try_get("score")?;
                let g = match score {
                    Some(_) => Some(Self::graham_score_from_row(r)?),
                    None => None,
                };
                Ok((company_from_row(r)?, g))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((page, total))
    }

    /// Export a company's prices to a Parquet file for portable archiving.
    pub async fn export_prices_parquet(&self, company_id: i64, path: &Path) -> Result<()> {
        let prices = self.get_prices(company_id).await?;
        let dates = StringArray::from_iter_values(prices.iter().map(|p| p.date.to_string()));
        let opens = Float64Array::from(prices.iter().map(|p| p.open).collect::<Vec<_>>());
        let highs = Float64Array::from(prices.iter().map(|p| p.high).collect::<Vec<_>>());
        let lows = Float64Array::from(prices.iter().map(|p| p.low).collect::<Vec<_>>());
        let closes = Float64Array::from(prices.iter().map(|p| p.close).collect::<Vec<_>>());
        let volumes = Int64Array::from(prices.iter().map(|p| p.volume).collect::<Vec<_>>());
        let sources = StringArray::from_iter_values(prices.iter().map(|p| p.source.as_str()));
        let schema = Arc::new(Schema::new(vec![
            Field::new("date", DataType::Utf8, false),
            Field::new("open", DataType::Float64, true),
            Field::new("high", DataType::Float64, true),
            Field::new("low", DataType::Float64, true),
            Field::new("close", DataType::Float64, false),
            Field::new("volume", DataType::Int64, true),
            Field::new("source", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(dates) as ArrayRef,
                Arc::new(opens) as ArrayRef,
                Arc::new(highs) as ArrayRef,
                Arc::new(lows) as ArrayRef,
                Arc::new(closes) as ArrayRef,
                Arc::new(volumes) as ArrayRef,
                Arc::new(sources) as ArrayRef,
            ],
        )
        .map_err(other)?;
        let file = std::fs::File::create(path).map_err(other)?;
        let mut writer = ArrowWriter::try_new(file, schema, None).map_err(other)?;
        writer.write(&batch).map_err(other)?;
        writer.close().map_err(other)?;
        Ok(())
    }
}
