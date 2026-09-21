# Metocast - Companion Module

Control Broadlink IR/RF devices and presentations through the Metocast desktop application via WebSocket.

## Prerequisites

- Metocast desktop application running
- Broadlink devices and/or Keynote configured in the Metocast app
- Auth token from Metocast settings

## Configuration

### Connection Settings

- **Host**: IP address or hostname of the computer running Metocast (default: `127.0.0.1`)
- **Port**: Metocast WebSocket port (default: `3737`)
- **Auth Token**: Authentication token from Metocast settings

## Actions

### RF/IR Commands

- **Execute RF/IR Command** — trigger a saved Broadlink command by slug
- **Execute Command by Category** — filter by category then trigger
- **Refresh Command List** — manually re-fetch the command list

### PPT Selector

- **PPT: Type Digit** — append a digit to the PPT file filter
- **PPT: Backspace** — remove the last digit from the filter
- **PPT: Clear Filter** — clear the filter entirely
- **PPT: Select File** — open the file at a display slot in Keynote
- **PPT: Select Folder** — switch to a different PPT folder
- **PPT: Refresh Files** — refresh the file list

### Presentation Control (Keynote)

- **Presentation: Open File** — open a presentation by path
- **Presentation: Start / Stop Slideshow** — Stop becomes Unload when Metocast is using the web presenter
- **Presentation: Close All / Close Latest**
- **Presentation: Next / Previous / First / Last Slide**
- **Presentation: Go to Slide** — jump to a specific slide number
- **Presentation: Toggle Blank Screen**
- **Presentation: Show Bible Reference** — show Textus or Lekció through the app WebSocket presenter. Leave Event set to “Backend selected event” to let Metocast choose the current/next event.

Song and Bible slides use the Classic or Editorial design selected in the Metocast Presentations settings. Classic shows an imported deck's source artwork; Editorial renders its extracted text in the selected design, and a live design change does not require reopening the deck.

## Presets

- **Show Textus** — sends `presenter.load_bible_reference` with `reference_type: "textus"` and no `event_id`, so the app backend chooses the event.
- **Show Lekcio** — sends `presenter.load_bible_reference` with `reference_type: "leckio"` and no `event_id`, so the app backend chooses the event.

## Feedbacks

| Feedback                       | Description                            |
| ------------------------------ | -------------------------------------- |
| Connection Status              | Green when connected to Metocast       |
| Command Available              | Blue when a specific command is loaded |
| PPT: Slot Has File             | Green when a PPT slot contains a file  |
| PPT: Filter Active             | Orange when a digit filter is applied  |
| Presentation: Slideshow Active | Green when a slideshow is running      |
| Presentation: Screen Blanked   | Black when the screen is blanked       |

## Variables

Use your Companion connection label as the variable prefix. For example, if the connection label is `Metocast`, use `$(Metocast:connection_status)`.

| Variable                                                    | Description                                   |
| ----------------------------------------------------------- | --------------------------------------------- |
| `$(<connection-label>:connection_status)`                   | Connected / Disconnected                      |
| `$(<connection-label>:last_command)`                        | Name of the last executed command             |
| `$(<connection-label>:command_count)`                       | Total available commands                      |
| `$(<connection-label>:ppt_filter)`                          | Current digit filter                          |
| `$(<connection-label>:ppt_match_count)`                     | Number of matching files                      |
| `$(<connection-label>:ppt_folder_name)`                     | Selected folder name                          |
| `$(<connection-label>:ppt_slot_1_name)` … `ppt_slot_5_name` | File names in slots 1–5                       |
| `$(<connection-label>:ppt_current_slide)`                   | Current slide number                          |
| `$(<connection-label>:ppt_total_slides)`                    | Total slides                                  |
| `$(<connection-label>:ppt_slideshow_active)`                | ON / OFF                                      |
| `$(<connection-label>:ppt_app)`                             | Presentation app name                         |
| `$(<connection-label>:ppt_backend)`                         | Active presentation backend                   |
| `$(<connection-label>:ppt_blanked)`                         | YES / NO                                      |
| `$(<connection-label>:presenter_event_count)`               | Number of backend-ordered presenter events    |
| `$(<connection-label>:presenter_selected_event)`            | Backend-selected current/next presenter event |

## Troubleshooting

### Module shows "Disconnected"

1. Verify Metocast is running
2. Check host and port settings
3. Verify the auth token matches Metocast settings
4. Check firewall allows connections on port 3737

### Commands not appearing

1. Ensure Broadlink commands are saved in Metocast
2. Click the **Refresh Command List** action button
