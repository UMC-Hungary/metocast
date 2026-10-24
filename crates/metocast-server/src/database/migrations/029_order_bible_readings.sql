-- A Bible step can read a passage of its own instead of the event's Textus or Lekció: its
-- reference and translation, and the verses the server looked up when it was saved, so the
-- service never waits on the network.
ALTER TABLE event_order_items
  ADD COLUMN reference TEXT,
  ADD COLUMN translation TEXT,
  ADD COLUMN verses JSONB;
