-- Announcements: an Order of Service step whose notes are bullet points, one per line,
-- which the server turns into slides when the step is shown.
ALTER TABLE event_order_items DROP CONSTRAINT event_order_items_kind_check;
ALTER TABLE event_order_items ADD CONSTRAINT event_order_items_kind_check
  CHECK (kind IN ('song', 'bible', 'block', 'announcements'));
