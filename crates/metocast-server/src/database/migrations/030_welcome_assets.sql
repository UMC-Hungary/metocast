-- The welcome screen's replaced photos ("01" to "12") and its logo ("logo"). Kept in the
-- database so they travel with backups and work on a headless core; the defaults are built into
-- the server.
CREATE TABLE welcome_assets (
  name TEXT PRIMARY KEY,
  mime TEXT NOT NULL,
  bytes BYTEA NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
