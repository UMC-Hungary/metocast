# Welcome screen

The slide on screen before and at the start of the service: a full-bleed photo, the church's
logo top left, a large greeting and a blessing line in the lower third, and the service's date.
Each month has its own photo.

It is what a **Slide** step of the [Order of Service](order-of-service.md) shows with the web
presenter. New events start with one, named after the greeting. The step's title is the
greeting, and its paragraphs (one per line) replace the blessing line. In Keynote mode a Slide
step shows the same text as a plain slide.

## The design

A modern, clean slide in the style of current streaming and editorial heroes:

- the church's logo top left (by default the cross and flame with _Nyíregyházi Metodista
  Gyülekezet_, in white with the red flame);
- the photo sharp and bright across the top two-thirds;
- in the lower third, the greeting (_Isten hozott._) large and left-aligned, the blessing under
  it, and the date on the right of the same line.

The type is Montserrat (SIL Open Font License), built into the server, which draws the text as
outlines. So the slide looks the same on every screen and none of them needs the font. There
are no shadows on the text. The photo zooms in slowly (8% over 20 seconds, then back) in the web
presenter and the apps. The Raspberry Pi receiver shows it still.

## Readable for everyone

- The text is white and large. The greeting is 14% of the slide's height, and nothing is
  smaller than 4.2% (about 30 pixels at 720p), so it reads from the back of the room.
- The photo makes room for the text instead of the text fighting the photo: the lower third
  melts into a soft, progressive blur, so no busy detail sits behind the letters.
- Where text sits (the lower third, and the logo's corner), bright parts of the photo are toned
  down: only the highlights, keeping each colour, so dark and middle tones stay as they are.
  Long gradients join this to the untouched rest of the photo, with no visible edge.
- Every pixel behind the text ends up dark enough for **4.5:1** contrast with white: WCAG's
  highest level (AAA) for large text. This is calculated for each photo, including any you
  upload, and a test checks all twelve built-in photos pixel by pixel.

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
- **Logo**: the church's logo is built in (_Default_). **Replace…** takes a PNG (transparency kept)
  or SVG; **Reset to Default** brings the built-in one back.

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
