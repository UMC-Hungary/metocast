# Order of Service and the slide library

An event's **Order of Service** is the service's steps in order, the way the printed sheet lists
them. Each step is one of three kinds:

- **Song**: a deck from the slide library. The hymn number goes in the title and the verses in the
  notes, e.g. *Ó, hála az Úrnak (47)* with *2. vers – közben kollekta*. A song can also have no
  deck, such as a closing verse sung without slides.
- **Bible reading**: the event's own *Lekció* or *Textus*. The verses are already stored on the
  event, so showing them needs no network.
- **Block**: a part with no slides, such as *Köszöntés, imádság*, *Igehirdetés* (the speaker goes
  in the notes) or *Hirdetések, áldás*.

During the service the operator steps through the list:

- a song opens its deck in the web presenter or Keynote;
- a reading loads the event's verses;
- a block leaves the screen as it is.

Every connected device sees the same list and the same current step.

## Use it

**Apple app**

- **Events → an event → Order of Service → Add/Edit Order of Service**:
  - **Add Step** offers *Song…* (picked from the library), *Song Without Slides*, the event's
    readings and *Block*.
  - Drag to reorder, swipe to delete.
  - **Start with the Readings** adds the Lekció and the Textus.
  - **Save**.
- **Live** shows the current event's Order of Service:
  - click a step to show it;
  - use **Previous Step** / **Next Step**.
  - The Dashboard's Presentation card links there.
- **Presentation** is the slide library:
  - every deck in the watched folders, with search by title, hymn number or lyrics and a
    folder filter (see [Slide library search](slide-library.md));
  - one click opens a deck;
  - the folder menu adds and removes watched folders.

**Sanctum**

- **Events**: the event's page has the same editor under the event form.
- **Home** runs the current event's Order of Service.
- **Presentations** has the full library under the quick search.

**Shared view**

`/ui/sanctum/presenter?view=order` shows the running Order of Service with the current step
highlighted. It is read-only, needs no login, and suits a stage screen or the people following
along.

**Companion**

- Actions: *Order of Service: Next Step* and *Previous Step*.
- Feedback: *Step On Screen*.
- Variables: `$(metocast:order_step)` and `$(metocast:order_step_number)`, e.g. `3/14`.
- With nothing running, Next Step starts the presenter's selected event at step 1.

## Entry points

| Does                    | REST                           | WebSocket        | MCP tool           |
| ----------------------- | ------------------------------ | ---------------- | ------------------ |
| Read an event's steps   | `GET /api/events/{id}/order`   |                  | `events_order_get` |
| Replace an event's steps | `PUT /api/events/{id}/order`  |                  | `events_order_set` |
| Show a step             |                                | `order.go`       | `order_go`         |
| Next / previous step    |                                | `order.next`, `order.previous` | `order_next`, `order_previous` |
| Which step is on screen |                                | `order.status`   | `order_status`     |

Pushes:

- `order.state` carries the step on screen and that event's steps. It is sent on connect and on
  every change.
- `event.order.changed` is sent when an event's list is replaced.

Notes:

- `PUT` replaces the whole list. Send each step's `id` back to keep it, so the step on screen
  stays current while you edit during the service.
- The live position is kept in memory, so a server restart starts with no step on screen.
- `order.status` is allowed without the token, like `presenter.status`, so the shared view works
  without logging in.
- The web presenter sends a whole deck, pictures included, to every device as one message of at
  most 64 MB. A larger deck is refused: the screen keeps showing the current deck, and the device
  that opened it gets an error saying how large the deck is. Every picture is embedded again on
  each slide that uses it, so the server first scales each picture down to the slide's size
  (1280 pixels on the long side) and stores opaque ones as JPEG: a 15 MB photo shown on four
  slides comes to under half a megabyte. Only decks with very many distinct pictures still
  reach the limit; shrink those in PowerPoint with *Picture Format → Compress Pictures*.

The slide library uses `GET /api/ppt/files`, `GET/POST /api/ppt/folders` and
`DELETE /api/ppt/folders/{id}`; [Slide library search](slide-library.md) describes the search
and its index.
