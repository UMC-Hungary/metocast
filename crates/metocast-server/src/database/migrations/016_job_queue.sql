-- ============================================================
-- 016_job_queue.sql
-- Generic durable job queue (SQS semantics in column form).
-- ============================================================

CREATE TABLE IF NOT EXISTS job_queue (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    queue         TEXT        NOT NULL,
    job_type      TEXT        NOT NULL,
    dedup_key     TEXT,
    payload       JSONB       NOT NULL DEFAULT '{}',
    status        TEXT        NOT NULL DEFAULT 'pending', -- pending|processing|succeeded|dead
    attempts      INT         NOT NULL DEFAULT 0,
    max_attempts  INT         NOT NULL DEFAULT 5,
    available_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at     TIMESTAMPTZ,
    locked_by     TEXT,
    last_error    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_job_queue_ready
    ON job_queue (queue, available_at)
    WHERE status = 'pending';

-- Coalesce rapid re-edits: at most one *pending* job per (queue, dedup_key).
-- Scoped to 'pending' only so an edit arriving mid-flight enqueues a fresh job
-- instead of mutating one the worker already holds.
CREATE UNIQUE INDEX IF NOT EXISTS idx_job_queue_dedup
    ON job_queue (queue, dedup_key)
    WHERE status = 'pending' AND dedup_key IS NOT NULL;

CREATE OR REPLACE FUNCTION notify_job_queue_change() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('queue_changed', COALESCE(NEW.queue, OLD.queue));
    RETURN COALESCE(NEW, OLD);
END; $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS job_queue_notify ON job_queue;
CREATE TRIGGER job_queue_notify
    AFTER INSERT OR UPDATE OR DELETE ON job_queue
    FOR EACH ROW EXECUTE FUNCTION notify_job_queue_change();
