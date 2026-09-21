-- ============================================================
-- 005_normalize_events.sql
-- Extract platform-specific fields into a generic event_connections table.
-- ============================================================

-- 1. Create event_connections table
CREATE TABLE event_connections (
    event_id        UUID        NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    platform        TEXT        NOT NULL,  -- 'youtube', 'facebook', …
    external_id     TEXT,                  -- broadcast_id / fb_event_id / …
    stream_url      TEXT,                  -- watch / stream URL
    event_url       TEXT,                  -- event page URL (Facebook, Discord, …)
    schedule_status TEXT        NOT NULL DEFAULT 'not_scheduled',
    privacy_status  TEXT,                  -- platform-specific value
    extra           JSONB,                 -- overflow (e.g. facebook stream_id)
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (event_id, platform)
);

-- 2. Migrate YouTube data
INSERT INTO event_connections (event_id, platform, external_id, stream_url, schedule_status, privacy_status)
SELECT id, 'youtube', youtube_broadcast_id, youtube_stream_url,
    COALESCE(youtube_schedule_status, 'not_scheduled'),
    COALESCE(youtube_privacy_status, 'private')
FROM events;

-- 3. Migrate Facebook data (facebook_stream_id goes into extra)
INSERT INTO event_connections (event_id, platform, external_id, event_url, schedule_status, privacy_status, extra)
SELECT id, 'facebook', facebook_event_id, facebook_event_url,
    COALESCE(facebook_schedule_status, 'not_scheduled'),
    COALESCE(facebook_privacy_status, 'EVERYONE'),
    CASE WHEN facebook_stream_id IS NOT NULL
         THEN jsonb_build_object('stream_id', facebook_stream_id)
         ELSE NULL END
FROM events;

-- 4. Drop the nine platform-specific columns from events
ALTER TABLE events DROP COLUMN youtube_broadcast_id;
ALTER TABLE events DROP COLUMN youtube_stream_url;
ALTER TABLE events DROP COLUMN youtube_schedule_status;
ALTER TABLE events DROP COLUMN youtube_privacy_status;
ALTER TABLE events DROP COLUMN facebook_event_id;
ALTER TABLE events DROP COLUMN facebook_stream_id;
ALTER TABLE events DROP COLUMN facebook_event_url;
ALTER TABLE events DROP COLUMN facebook_schedule_status;
ALTER TABLE events DROP COLUMN facebook_privacy_status;

-- 5. Update notify_event_change() to include connections via subquery
CREATE OR REPLACE FUNCTION notify_event_change() RETURNS trigger AS $$
DECLARE
    full_record json;
BEGIN
    IF current_setting('app.skip_sync_notify', true) = 'true' THEN
        RETURN NEW;
    END IF;
    SELECT row_to_json(r) INTO full_record FROM (
        SELECT e.*,
            COALESCE(
                (SELECT json_agg(json_build_object(
                    'platform', c.platform,
                    'external_id', c.external_id,
                    'stream_url', c.stream_url,
                    'event_url', c.event_url,
                    'schedule_status', c.schedule_status,
                    'privacy_status', c.privacy_status,
                    'extra', c.extra
                ) ORDER BY c.platform)
                FROM event_connections c WHERE c.event_id = e.id),
                '[]'::json
            ) AS connections
        FROM events e
        WHERE e.id = NEW.id
    ) r;
    PERFORM pg_notify('event_changes', json_build_object(
        'operation', TG_OP, 'record', full_record
    )::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 6. Trigger on event_connections that fires event_changes with the full event
CREATE OR REPLACE FUNCTION notify_event_connection_change() RETURNS trigger AS $$
DECLARE
    full_record json;
BEGIN
    IF current_setting('app.skip_sync_notify', true) = 'true' THEN
        RETURN NEW;
    END IF;
    SELECT row_to_json(r) INTO full_record FROM (
        SELECT e.*,
            COALESCE(
                (SELECT json_agg(json_build_object(
                    'platform', c.platform,
                    'external_id', c.external_id,
                    'stream_url', c.stream_url,
                    'event_url', c.event_url,
                    'schedule_status', c.schedule_status,
                    'privacy_status', c.privacy_status,
                    'extra', c.extra
                ) ORDER BY c.platform)
                FROM event_connections c WHERE c.event_id = e.id),
                '[]'::json
            ) AS connections
        FROM events e
        WHERE e.id = NEW.event_id
    ) r;
    PERFORM pg_notify('event_changes', json_build_object(
        'operation', 'UPDATE', 'record', full_record
    )::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER event_connections_notify
    AFTER INSERT OR UPDATE ON event_connections
    FOR EACH ROW EXECUTE FUNCTION notify_event_connection_change();
