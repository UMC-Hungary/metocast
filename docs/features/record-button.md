# The Record button

One button that starts and stops every recorder you choose: OBS, the ATEM, the Blackmagic camera,
Middle Control (every camera, or one), the RØDECaster's own SD or USB recorder, and Metocast's
audio recording (FLAC). It shows what each recorder is doing, and Metocast tells you every time a
recording stops.

## Use it

- **Choose the recorders:** Settings → **Record Button** in the Apple app. The server keeps the
  choice, so every Mac and iPhone drives the same recorders. Until you choose, the button records
  Metocast's audio only, as it always did.
- **Record:** the **Record** button is in the Mac toolbar on every page, and in the Live tab's bar on
  the iPhone. A press starts every chosen recorder at once. A recorder that is already recording is
  left alone.
- **Stop:** while any chosen recorder is recording, a press asks first, then stops them all.
- **Status:** the button's menu (the arrow beside it on the Mac, or a long press on the iPhone) has
  a row for each chosen recorder: **Recording**, **Not recording** or **Not connected**, and why its
  last start failed, for example "Didn't start: There's no card or SSD in the camera." A start by an
  automation shows here too. **Start the Others** retries the ones that aren't recording.
- **The button's symbol** fills while anything records. It turns red when every chosen recorder is
  recording, and orange when one isn't connected or failed to start.

### When a recording stops

Whenever any recording stops, chosen or not, Metocast adds it to the Notifications list and shows a
system notification. The notification names the recorder and the reason:

- **stopped from Metocast:** the Record button, a Live page button or an automation stopped it;
- **the connection dropped:** Metocast lost the device;
- **stopped on the device:** it stopped by itself, or someone stopped it on the device. For the
  camera, the notification adds that its card or SSD may be full or too slow. That is how a camera
  recording that ended because the drive couldn't keep up shows up.

The server looks at the recorders once a second, using the state the connectors already have, so
watching them costs the devices nothing.

## Entry points

| Does                                | REST                                | WebSocket         | MCP tool                     |
| ----------------------------------- | ----------------------------------- | ----------------- | ---------------------------- |
| Read the chosen recorders           | `GET /api/settings/record-button`   |                   | `settings_record_button_get` |
| Choose the recorders                | `PUT /api/settings/record-button`   |                   | `settings_record_button_set` |
| Every recorder with its state       | `GET /api/recorders`                |                   | `recorders_list`             |
| Start the chosen recorders          |                                     | `recorders.start` | `recorders_start`            |
| Stop the chosen recorders           |                                     | `recorders.stop`  | `recorders_stop` (destructive) |

Pushes: `recorders.state` (every recorder: chosen, connected, recording, last start) when anything
changes and to a client with the token when it connects, and `recorder.stopped`
(`target`, `reason`: `metocast` / `disconnected` / `device`, `error`) whenever a recording stops.
A failed start or stop answers `recorders_failed: blackmagic-camera: camera_no_media; …`.

Starting and stopping go through the same code as automations (`recording_control::ensure`).

In Companion: the action **Record Button: Start/Stop** and the feedback **Record Button: Recording**.
