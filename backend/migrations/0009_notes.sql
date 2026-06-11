CREATE TABLE IF NOT EXISTS notes (
    user_id    INTEGER  NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    company_id INTEGER  NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    body       TEXT     NOT NULL,
    updated_at DATETIME NOT NULL,
    PRIMARY KEY (user_id, company_id)
);
