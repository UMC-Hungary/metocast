# Welcome screen

The slide on screen before and at the start of the service: a full-bleed photo, the church's
logo top right, a large greeting, a blessing line, and the church name with the service's date.
Each month has its own photo.

It is what a **Slide** step of the [Order of Service](order-of-service.md) shows with the web
presenter. New events start with one, named after the greeting. The step's title is the
greeting, and its paragraphs (one per line) replace the blessing line. In Keynote mode a Slide
step shows the same text as a plain slide.

## Readable for everyone

- The text is white, bold and large: the greeting is 11% of the slide's height, the other lines 5%.
- Behind it, a dark overlay is sized for each photo. The server measures how bright the photo is
  behind the text, ignoring only the brightest 5% of specks. It then darkens just enough that the
  white text has at least **4.5:1** contrast there: WCAG's highest level (AAA) for large text.
- A dark photo gets a light touch and a bright one (snow, sky, blossom) a stronger one, and a
  thin shadow keeps letter edges crisp.
- A replaced photo goes through the same calculation, so no photo can make the text unreadable.
  A test checks the twelve built-in photos.

## The photos

There are 12 photos, one for each month: the service's month picks the photo (photo 1 is January's).
The defaults follow the seasons: snow in January and February, blossom in spring, summer fields, and
autumn trees in October and November. They are public-domain (CC0) photos from Wikimedia Commons,
most first published on Unsplash;
[`CREDITS.md`](../../crates/metocast-server/assets/welcome/CREDITS.md) lists each one's source.

## Use it

**Apple app**: **Presentation → Presentation Settings** (a page inside Presentation) **→ Welcome
Screen**:

- **Greeting**, **Blessing** and **Church Name** save themselves as you type;
- **Language** (Magyar / English) sets how the date is written (_2026. október 25. vasárnap_ or
  _Sunday, October 25, 2026_). Switching it also swaps in that language's default greeting and
  blessing, unless you wrote your own;
- the photos are listed by month. Click one to **preview** that month's slide as the screens will
  show it. From the preview, **Replace…** takes any photo from this computer (cropped to 16:9) and
  **Reset to Default** brings back the built-in one. A pencil mark shows which are replaced;
- **Logo**: **Choose…** a PNG (transparency kept) or SVG, or **Remove** it. The cross and flame
  is the United Methodist Church's registered emblem, so Metocast doesn't ship it: use your
  church's official file.

The slide shows on every screen that shows slides: the web presenter (`/ui/sanctum/presenter`),
the Mac and iPhone app's Presentation page, and the Raspberry Pi presenter receiver. The server
sends it as a finished SVG with the text already drawn, so no screen needs the fonts.

## Entry points

| Does                          | REST                                             | MCP tool                        |
| ----------------------------- | ------------------------------------------------ | ------------------------------- |
| Read the settings             | `GET /api/presenter/welcome`                     | `presenter_welcome_get`         |
| Change the text or language   | `PUT /api/presenter/welcome`                     | `presenter_welcome_set`         |
| Get a month's photo (1–12)    | `GET /api/presenter/welcome/photos/{n}`          |                                 |
| Preview a month's slide (SVG) | `GET /api/presenter/welcome/preview/{n}`         |                                 |
| Replace a photo (image bytes) | `PUT /api/presenter/welcome/photos/{n}`          |                                 |
| Reset a photo                 | `DELETE /api/presenter/welcome/photos/{n}`       | `presenter_welcome_photo_reset` |
| Get, set or remove the logo   | `GET`/`PUT`/`DELETE /api/presenter/welcome/logo` | `presenter_welcome_logo_remove` |

The slide itself arrives as the usual `presenter.state` push (`renderMode: "svg"`). Replaced photos and the logo are kept in the database, so they travel with
backups.
