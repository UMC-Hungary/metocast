-- Repair event_connections rows where external_id is set but schedule_status
-- was incorrectly left as 'not_scheduled' or overwritten with 'failed' by the
-- auto-scheduling loop attempting to reschedule already-scheduled past events.
UPDATE event_connections
SET    schedule_status = 'scheduled',
       updated_at      = NOW()
WHERE  external_id IS NOT NULL
  AND  schedule_status != 'scheduled';
