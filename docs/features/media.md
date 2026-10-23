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

### A clip whose end is damaged

A Blackmagic RAW recording that didn't end cleanly (the card pulled or the power cut mid-write)
can count frames at its end that were never fully written. Reading one fails with
`reading frame N failed (0x8000FFFF)`. The probe reads the clip's last two seconds frame by frame,
so the Files card warns before the job starts. The render decodes up to the first unreadable
frame and holds the last good one over the rest, instead of failing at the very end: the picture
keeps its length, so the program's sound and transcripts come out exactly as for an undamaged clip.
The finished job says how many frames were held (`damagedFrames` in the program's `report`).
Unreadable frames earlier in the clip still fail the job; a frozen picture there would hide a
real problem.

### Transcripts

To caption a program with the live English translation:

1. After the service, open **Settings → Server Tools → Transcripts**, open the session and
   **Download** it as `.vtt` (or `.srt`). Use the cleaned-up text if you ran the AI clean-up.
2. On the Media page, drop the video, the transcript and, if there is one, the audio recording.
   Pick **English** for the transcript on the Subtitles card.
3. Start the job.

The transcript can count from the start of the live session: the job finds where the program sits
in it. After the picture and sound are done, the **Placing the transcript on the speech** step:

- rebuilds the live text into whole sentences. The live translation arrives 2–6 s late, in
  fragments and with stray quote marks;
- has Whisper (bundled in the app) translate the program's own audio with a time for every word;
- starts each sentence when its first word is spoken.

The Finished card then says, for example, "English transcript: 239 sentences placed on the speech,
225 matched by word". The first such job downloads Whisper's model (3.1 GB); the Files card says
so when an English transcript is dropped. The step takes about a fifth of the program's length on
an M1.

Other languages, or an English transcript when Whisper can't run or the text doesn't match the
speech, are moved as a whole to where their lines match the program's voice. A live translation
rarely matches clearly that way. When the match isn't clear and the transcript runs past the end
of the program, it must be timed to the recording, so it moves with the recording, to where the
sync put it, and the Finished card says to check it. A transcript that fits within the program is
left as it was.

Each language is saved beside the program as `<name>.<language>.srt` (`subtitles` in the
program's `report`: `onSpeech` and `matched` when it was placed sentence by sentence). The step
is `captions` in `media.program.progress`.

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
