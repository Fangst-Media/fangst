-- One row per download, whatever the protocol. Protocol details live in their own tables so the
-- queue, API and UI never need to know which protocol a job uses.
CREATE TABLE jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('usenet', 'torrent')),
    name TEXT NOT NULL,
    category TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'completed', 'failed')),
    error TEXT,
    output_path TEXT,
    total_bytes INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    finished_at INTEGER
);

CREATE INDEX jobs_by_queue_order ON jobs (status, priority DESC, id);

CREATE TABLE usenet_jobs (
    job_id INTEGER PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    nzb BLOB NOT NULL
);

-- A row means that segment's data is on disk, so a restart skips it.
CREATE TABLE usenet_segments_done (
    job_id INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    file_index INTEGER NOT NULL,
    number INTEGER NOT NULL,
    PRIMARY KEY (job_id, file_index, number)
) WITHOUT ROWID;
