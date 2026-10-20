# Broadcast controls

Each event can have a scheduled **YouTube broadcast** and a **Facebook live video**. Metocast
shows, in the events list, whether each exists and what the platform says about it, and takes it
live or ends it, without opening YouTube Studio or Facebook Live Producer.

## States

The badge on an event shows one state per platform. It is the platform's own state, not
Metocast's guess:

| Badge | Means | YouTube | Facebook |
|---|---|---|---|
| **Scheduled** | Created, waiting | `created`, `ready` | `SCHEDULED_UNPUBLISHED`, `UNPUBLISHED` |
| **Starting** | Going live | `testStarting`, `testing`, `liveStarting` | published, waiting for video |
| **Live** | Viewers see it | `live` | `LIVE` |
| **Ended** | Over; can't be restarted | `complete`, `revoked` | `LIVE_STOPPED`, `VOD`, … |

The same value is in each connection's `scheduleStatus`. Before a broadcast exists it reads
`not_scheduled`, or `failed` when scheduling failed.

### How it stays current

- Going live or ending updates the badge right away.
- While an event is within 12 hours of now, the server asks the platforms every minute about its
  scheduled, starting and live broadcasts. That catches a broadcast started or ended in YouTube
  Studio, or ended by YouTube itself. One YouTube call covers every broadcast (1 quota unit).
  Facebook gets one call per live video. With no service near, nothing is called.
- Every change reaches every client as `event.changed`. Clients never poll.

## Go live

**Start OBS (or your encoder) streaming first.** Going live then:

- **YouTube**:
  - binds the broadcast to the channel's stream when nothing is bound yet (the stream OBS, the
    ATEM and the camera push to);
  - refuses with **"No video is reaching YouTube yet"** (`encoder_not_streaming`) while that
    stream gets no video, and changes nothing;
  - otherwise goes through *testing* to *live*. This takes up to about 30 seconds and shows
    **Starting** meanwhile. It costs about 150 of the daily 10 000 quota units.
- **Facebook** publishes the live video at once. It shows **Starting** until video arrives, then
  **Live**.

**End** stops the broadcast for viewers. An ended broadcast can't be restarted; schedule a new
one.

Both ask for confirmation first.

## Use it

**Apple app (Mac, iPhone, iPad)**

- **Events** and the **Dashboard**: each row shows the badges on Liquid Glass.
- To go live or end, use the row's context menu (right-click, or long-press on iPhone), or swipe
  the row right on iPhone. Tapping the row still opens the event.
- **An event → Streaming** shows the same badges, with **Go Live on …** and **End … Broadcast**
  buttons.
- **Event editor → YouTube**: when no enabled scheduled job pulls YouTube, a small note says so.
  It also says so when **Auto-upload** is on and no enabled job uploads. The ⓘ explains what a
  scheduled job does, and **Set Up…** opens **Server Tools → Scheduled Jobs**.

**Sanctum**

- **Events**: each row shows the badges.
- The selected event's panel has **Go live on …** / **End … broadcast** buttons, each with a
  confirmation dialog.
- **Event editor**: the same scheduled-job note sits under *Privacy*, with a tooltip and a link
  to Settings.

**Companion**

- Action **Event: Go Live / End Broadcast**: platform, operation, and the event (default: the
  current event).
- Feedback **Event: Broadcast Live**: lit while that platform's broadcast is live.

## Entry points

| | Go live | End |
|---|---|---|
| REST | `POST /api/events/{id}/stream/{platform}/start` | `POST /api/events/{id}/stream/{platform}/stop` |
| WebSocket | `events.stream.start` `{ event_id, platform }` | `events.stream.stop` `{ event_id, platform }` |
| MCP | `events_stream_start` (destructive) | `events_stream_stop` (destructive) |

`platform` is `youtube` or `facebook`. REST answers `{ "ok": true }`; WebSocket answers `ok`.
Refusals:

| Code | REST | Means |
|---|---|---|
| `not_found` | 404 | No such event |
| `not_authenticated` | 401 | Not signed in to that platform |
| `not_scheduled` | 409 | The event has no broadcast there |
| `already_live` / `not_live` | 409 | Already live / nothing to end |
| `broadcast_ended` | 409 | It has ended; schedule a new one |
| `encoder_not_streaming` | 409 | YouTube gets no video yet |
| `youtube_api_…` / `facebook_api_…` | 502 | The platform refused, with its message |

The state is read through `GET /api/events` (each summary's `connections[].scheduleStatus`),
and changes arrive as `event.changed`.
