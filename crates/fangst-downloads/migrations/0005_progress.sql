-- Bytes of the job already on disk, written while it runs and when it stops, so a restart shows
-- where the job was. Live progress between writes is kept in memory only.
ALTER TABLE jobs ADD COLUMN downloaded_bytes INTEGER NOT NULL DEFAULT 0;
