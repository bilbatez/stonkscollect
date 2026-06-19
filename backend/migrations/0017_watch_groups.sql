-- Named watch groups ("tags"): a user can organize watched companies into
-- groups, and a company may belong to many of a user's groups.
CREATE TABLE watch_groups (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    name    TEXT NOT NULL,
    UNIQUE (user_id, name)
);

CREATE TABLE watch_group_members (
    group_id   INTEGER NOT NULL REFERENCES watch_groups(id) ON DELETE CASCADE,
    company_id INTEGER NOT NULL REFERENCES companies(id),
    PRIMARY KEY (group_id, company_id)
);
