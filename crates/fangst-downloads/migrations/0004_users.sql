-- People who may use the daemon, how they prove it, and secrets the daemon keeps for them.
-- Nothing here refers to jobs or protocol tables, so it applies the same with or without them.
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- Argon2id in PHC form. NULL until a password is set; such a user signs in with an API key only.
    password_hash TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- `environment` is the key the operator passes at start; it is replaced on every start.
    name TEXT NOT NULL,
    -- Keyed hash of the key, so a request can be matched without decrypting every row.
    lookup TEXT NOT NULL UNIQUE,
    -- The key itself, encrypted under the master key so it can be shown to its owner again.
    sealed BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (user_id, name)
);

-- Only a hash of each session token is stored; a copy of the database can't be used to sign in.
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE INDEX sessions_by_expiry ON sessions (expires_at);

-- Usenet server passwords and other credentials, encrypted under the master key.
CREATE TABLE secrets (
    name TEXT PRIMARY KEY,
    sealed BLOB NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;
