# Welcome screen

The slide on screen before and at the start of the service: a full-bleed photo, the church's
logo top right, a large greeting, a blessing line, and the church name with the service's date.
Twelve photos take turns while it is showing.

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

The defaults are 12 public-domain (CC0) photos from Wikimedia Commons, most first published on
Unsplash; [`CREDITS.md`](../../crates/metocast-server/assets/welcome/CREDITS.md) lists each
source. While the welcome step is on screen, the photo changes every 20 seconds (adjustable).
Each service starts on a different one.

## Use it

**Apple app**: **Presentation → Presentation Settings** (a page inside Presentation) **→ Welcome
Screen**:

- **Greeting**, **Blessing** and **Church Name** save themselves as you type;
- **Next photo after … s** sets the interval;
- click a photo for **Replace…** (any photo from this computer, cropped to 16:9) or **Reset to
  Default**. A pencil mark shows which are replaced;
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
| Change the text or interval   | `PUT /api/presenter/welcome`                     | `presenter_welcome_set`         |
| Get a photo (1–12)            | `GET /api/presenter/welcome/photos/{n}`          |                                 |
| Replace a photo (image bytes) | `PUT /api/presenter/welcome/photos/{n}`          |                                 |
| Reset a photo                 | `DELETE /api/presenter/welcome/photos/{n}`       | `presenter_welcome_photo_reset` |
| Get, set or remove the logo   | `GET`/`PUT`/`DELETE /api/presenter/welcome/logo` | `presenter_welcome_logo_remove` |

The slide itself arrives as the usual `presenter.state` push (`renderMode: "svg"`), every time
the photo changes. Replaced photos and the logo are kept in the database, so they travel with
backups.
