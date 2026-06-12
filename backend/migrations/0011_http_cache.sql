-- Per-URL HTTP cache validators for conditional GETs (If-None-Match /
-- If-Modified-Since). A 304 lets a collector skip re-downloading and
-- re-parsing an unchanged upstream document.
CREATE TABLE http_cache (
    url           TEXT PRIMARY KEY,
    etag          TEXT,
    last_modified TEXT,
    fetched_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
