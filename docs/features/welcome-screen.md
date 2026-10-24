# Welcome screen

The slide on screen before and at the start of the service: a full-bleed photo, the church's
logo top right, a large greeting, a blessing line, and the church name with the service's date.
Each month has its own photo.

It is what a **Slide** step of the [Order of Service](order-of-service.md) shows with the web
presenter. New events start with one, named after the greeting. The step's title is the
greeting, and its paragraphs (one per line) replace the blessing line. In Keynote mode a Slide
step shows the same text as a plain slide.

## The design

One big, calm greeting on a full photo, with small anchors in the corners:

- the greeting in sentence case (_Isten hozott._) in the middle, with the blessing under it;
- the church name top left, the logo top right;
- the date bottom left, as a large day number beside the month and weekday;
- the year bottom right.

The type is Montserrat (SIL Open Font License), built into the server: it draws the text as
outlines, so the slide looks the same on every screen and none of them needs the font. The photo
zooms in slowly (8% over 20 seconds, then back) in the web presenter and the apps. The Raspberry
Pi receiver shows it still.

## Readable for everyone

- The text is white and large. The greeting is 15% of the slide's height, and nothing is
  smaller than 4.2% (about 30 pixels at 720p), so it reads from the back of the room.
- The server measures how bright the photo is behind each group of text: the greeting and
  blessing, the top corners, and the bottom corners. It ignores only the brightest 5% of specks.
- It shades the whole photo evenly just enough for the greeting. Then it adds a soft gradient at
  the top or bottom only where the corner text needs more.
- Every line then has at least **4.5:1** contrast: WCAG's highest level (AAA) for large text. A
  dark photo stays almost untouched; a bright one (snow, sky, sunflowers) is shaded more, with no
  visible bands or shapes.
- A replaced photo goes through the same calculation, so no photo can make the text unreadable.
  A test checks all twelve built-in photos, zone by zone.

## The photos

There are 12 photos, one for each month: the service's month picks the photo (photo 1 is January's).
The defaults were all taken in Hungary:

1. Mátraszentimre in snow
2. the snowy Mátra
3. rural Hungary at Easter
4. blossom in Zalaegerszeg
5. a poppy field near Milejszeg
6. lavender at Tihany
7. sunflowers in the Hungarian countryside
8. a sunset at Gyenesdiás
9. vineyards at Hegymagas
10. autumn in Szeged
11. Budapest in morning fog
12. Budapest in snow

They come from Unsplash under the Unsplash License;
[`CREDITS.md`](../../crates/metocast-server/assets/welcome/CREDITS.md) names each photographer.

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
