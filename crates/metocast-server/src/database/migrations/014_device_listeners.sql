CREATE TABLE IF NOT EXISTS device_listeners (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    connector_type TEXT NOT NULL DEFAULT 'obs',
    category TEXT NOT NULL,
    device_item_value TEXT NOT NULL,
    device_item_name TEXT NOT NULL,
    friendly_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
