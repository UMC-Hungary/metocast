-- Rules that run inside the core: when one device does something, Metocast does something on
-- others (see crates/metocast-core/src/automations.rs).
--
-- Triggers and actions are tagged unions, so they live as JSONB and are checked by serde, not
-- spread over feature tables the way cron_jobs does it.
CREATE TABLE automations (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name             TEXT        NOT NULL,
    enabled          BOOLEAN     NOT NULL DEFAULT true,
    triggers         JSONB       NOT NULL,
    actions          JSONB       NOT NULL,
    cooldown_seconds REAL        NOT NULL DEFAULT 5,
    require_event    BOOLEAN     NOT NULL DEFAULT false,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The master switch: nothing fires while it is off.
INSERT INTO app_settings (key, value) VALUES ('automations_enabled', 'true')
ON CONFLICT (key) DO NOTHING;
