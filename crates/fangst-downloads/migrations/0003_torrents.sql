-- Torrent details beside the protocol-neutral jobs row. A magnet job starts with only the link;
-- the metadata is stored once a peer sends it, so a restart never has to ask for it again.
CREATE TABLE torrent_jobs (
    job_id INTEGER PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    -- Lowercase hex of the v1 info hash.
    info_hash TEXT NOT NULL,
    magnet TEXT,
    metainfo BLOB,
    CHECK (magnet IS NOT NULL OR metainfo IS NOT NULL)
);

-- One engine runs one copy of a torrent, so the same torrent can't be two jobs.
CREATE UNIQUE INDEX torrent_jobs_by_info_hash ON torrent_jobs (info_hash);
