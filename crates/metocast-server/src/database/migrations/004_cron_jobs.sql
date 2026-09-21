CREATE TABLE cron_jobs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT        NOT NULL,
    cron_expression TEXT        NOT NULL,
    enabled         BOOLEAN     NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One row per enabled feature per job.
-- Currently supported features: 'pull_youtube'
CREATE TABLE cron_job_features (
    cron_job_id UUID NOT NULL REFERENCES cron_jobs(id) ON DELETE CASCADE,
    feature     TEXT NOT NULL,
    PRIMARY KEY (cron_job_id, feature)
);
