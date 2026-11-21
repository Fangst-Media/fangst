-- Append-only history of each job, so a user can see why a download ended the way it did.
CREATE TABLE job_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    message TEXT NOT NULL
);

CREATE INDEX job_events_by_job ON job_events (job_id, id);
