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
