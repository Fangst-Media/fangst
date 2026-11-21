-- What seeding has counted for a torrent job, written while it seeds and when it stops, so a
-- restart resumes with the upload and seed time already spent.
ALTER TABLE torrent_jobs ADD COLUMN uploaded_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE torrent_jobs ADD COLUMN seeding_secs INTEGER NOT NULL DEFAULT 0;
-- 1 while a completed job still seeds; a restart puts these back in the engine. Jobs completed
-- before seeding existed stay 0, as they stopped uploading when they finished.
ALTER TABLE torrent_jobs ADD COLUMN seeding INTEGER NOT NULL DEFAULT 0 CHECK (seeding IN (0, 1));

-- Per-category seeding limits. NULL uses the default; a negative value turns that limit off.
ALTER TABLE categories ADD COLUMN seed_ratio_limit REAL;
ALTER TABLE categories ADD COLUMN seed_time_limit_secs INTEGER;
