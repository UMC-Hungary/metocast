# Media processing

Turns raw service recordings into files ready to upload: a graded 6K HEVC master of the sermon
from the camera's Blackmagic RAW clip, a clean voice track, or both merged into one program. Jobs
run on the server's `media` queue, one at a time, and finished files are filed under an event (or
Unassigned Recordings) where the upload flow picks them up.

The details, settings and bundled tools are in the
[README](../../README.md#media-processing-video-and-audio-masters).

## Use it

**Media** in the Mac app's sidebar, in server mode (it reads files from the server's own disk).
Drop the video, its audio recording, a transcript or a Resolve look on the Files card, adjust the
look on the viewer's tiles, pick the event, and start the job. Settings are under **Connectors →
Media Processing**; changes on the page apply to that job unless saved as the default.

### While a job runs

- **One at a time.** The Start card swaps its button for the running job's progress and
  **Cancel**; the next job starts once it ends. The server's queue runs one job at a time anyway.
- **Go anywhere.** Jobs belong to the app's connection, not the page: a progress ring in the
  window's toolbar follows you to every page (click it for progress and Cancel), and the Media page
  shows the job again when you come back.
- **Results on top.** A finished, failed or cancelled job lands in **Finished** at the top of the
  page, newest first, and the page scrolls up to it.
- **A notification when it ends.** The first job asks permission; finished and failed jobs then
  post a system notification with the file name, and clicking it opens the Media page. There is none
  while the Media page is already in front: the result is on screen.
- **Quitting asks first.** The job runs on the server, which in server mode is the app's own helper,
  so quitting would stop it (and the server would start it over at the next launch). Quitting during
  a job offers **Cancel Export and Quit** or **Keep Running**. Closing the window is fine: the app,
  and the job, keep running. The iPhone has no Media page; a job started elsewhere carries on in the
  Mac's server regardless of the phone.

Blackmagic RAW also needs the free Blackmagic RAW install on that Mac, which holds the framework
the app develops clips with. A job that stops with `this Mac is missing: braw-decode` means the
opposite: the app itself was built without Blackmagic RAW support, which no install can repair —
see [the SDK on a build machine](../../braw-decode/README.md#the-sdk-on-a-build-machine).

## Entry points

| Does                                      | REST                                                                                     | MCP tool                         |
| ----------------------------------------- | ---------------------------------------------------------------------------------------- | -------------------------------- |
| Describe a video file                     | `POST /api/media/probe`                                                                  | `media_probe`                    |
| Render a still preview of the graded look | `POST /api/media/preview`                                                                | `media_preview`                  |
| Start a video render                      | `POST /api/media/renders`                                                                | `media_renders_start`            |
| Cancel a render                           | `POST /api/media/renders/{id}/cancel`                                                    | `media_renders_cancel`           |
| Describe the audio files of a recording   | `POST /api/media/audio/probe`                                                            | `media_audio_probe`              |
| Start an audio master                     | `POST /api/media/audio`                                                                  | `media_audio_start`              |
| Describe a program (video plus audio)     | `POST /api/media/programs/probe`                                                         | `media_programs_probe`           |
| Start a program render                    | `POST /api/media/programs`                                                               | `media_programs_start`           |
| Shift a program's audio against its video | `POST /api/media/programs/{id}/nudge`                                                    | `media_programs_nudge`           |
| Read or save the processing defaults      | `GET`/`PUT /api/connectors/media/config`                                                 | `connectors_config_get`/`_set`   |
| Watch job progress                        | WebSocket push `media.render.progress`, `media.audio.progress`, `media.program.progress` | `events_poll`                    |
| See and retry the jobs                    | `GET /api/queues/media/jobs`, `POST /api/jobs/{id}/retry`                                | `queues_jobs_list`, `jobs_retry` |

Progress and results arrive as push messages; poll them with `events_poll` and the `types` filter,
or read `metocast://state/queues`.
