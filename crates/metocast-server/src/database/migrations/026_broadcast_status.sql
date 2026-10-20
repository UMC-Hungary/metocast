-- ============================================================
-- 026_broadcast_status.sql
-- schedule_status held YouTube's raw lifeCycleStatus once the pull job had run.
-- Every platform now shares one vocabulary: not_scheduled, failed, scheduled,
-- starting, live, ended.
-- ============================================================

UPDATE event_connections
SET schedule_status = CASE
        WHEN schedule_status IN ('created', 'ready') THEN 'scheduled'
        WHEN schedule_status IN ('testStarting', 'testing', 'liveStarting') THEN 'starting'
        WHEN schedule_status IN ('complete', 'revoked') THEN 'ended'
        ELSE schedule_status
    END
WHERE schedule_status IN ('created', 'ready', 'testStarting', 'testing', 'liveStarting', 'complete', 'revoked');
