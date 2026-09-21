# Presenter theme direction spec

## Product and objective

Metocast is a church livestream control application built as a Tauri desktop app with two Svelte user interfaces. The requested change concerns the built-in web presenter: the full-screen browser output that currently renders text extracted from a PowerPoint deck and Bible references assembled by the backend. The visual reference is the live local prototype at `http://localhost:5173/`, specifically the 16:9 preview inside its Presenter screen. The target is not a generic presentation editor and not a new control dashboard. It is a selectable visual treatment for the projected slide itself. The production implementation will eventually add a design switch for Bible presentation while making the same design language usable for song lyrics. These direction drafts are an approval gate only; they must not change production Svelte, TypeScript, Rust, protocol, or locale files.

## Audience and viewing context

The primary audience is a congregation reading from a projector or large television at roughly 5–15 metres. The operator sees a smaller preview, but legibility on the projected output is the first constraint. The visual tone should feel reverent, quiet, warm, crafted, and contemporary without looking like generic worship software. It must remain calm behind Hungarian text with accents and relatively long lines. A 16:9 canvas is mandatory. Song and Bible layouts must clearly belong to the same family while using different typographic structures that fit their content.

## Shared content

Every direction must show both of these real slide types together on one comparison page so the user can judge the system rather than a single cover slide.

Song slide content:

- Eyebrow: `ÉNEK · KEGYELEMBŐL`
- Main lyric lines: `Kegyelemből léphetünk be, szent jelenlétedbe,` and `nem saját érdemeinkből, csak Jézus vérével.`
- Section label: `1. VERSZAK`
- Small church mark: `METOCAST`

Bible slide content:

- Eyebrow: `RÓMA 8:1 · RÚF`
- Verse number: `1`
- Verse text: `Nincs tehát most már semmiféle kárhoztató ítélet azok ellen, akik Krisztus Jézusban vannak.`
- Small church mark: `METOCAST`

## Reference language that all directions must preserve

The reference slide uses a near-black brown surface (`#11100e`), warm ivory type (`#f4efe3`), antique-gold accents (`#cfab69`), a delicate radial gold glow in the upper-right, and a faint diagonal light veil. Its typography pairs a literary high-contrast serif similar to Cormorant Garamond with a spaced uppercase mono label similar to Geist Mono. The song title is centred and generous. The Bible composition is asymmetric: the verse number sits in a narrow left column, the passage text sits to the right, and the citation is a tiny gold mono line above. The church mark sits unobtrusively in the lower-right. The output should feel like editorial book typography translated to a dark projection surface. Avoid icons, cards, control chrome, page counters, fake film grain, photos, illustrations, bright gradients, rounded UI panels, and anything that reads as a SaaS dashboard. The slide is the artwork.

## Comparison-page format

Create a single self-contained HTML file for each direction. The browser page may have a quiet neutral comparison background and a short direction label, but its two primary objects must be 16:9 slide canvases shown side-by-side or in a clean two-row composition. The slide canvas itself must be scalable and use only local CSS/system font fallbacks, with no network requests. Add concise assumptions in an HTML comment at the top. Both slide canvases must remain fully legible in a 1440×900 screenshot, with no clipping. A small caption may identify `Song` and `Bible`; no explanatory marketing copy is needed.

## Direction assignments

### A — Faithful editorial

Use the skill roulette result for this task: PPT style 17, Editorial Longform. Keep the closest visual fidelity to the provided prototype. Preserve its warm-black, ivory, gold, serif-plus-mono grammar and its centred-song/asymmetric-scripture distinction. The form comes directly from the reference and from the editorial character of scripture and hymnody.

### B — Projection clarity

Use the same reference as the real-world benchmark, but tune it for long-distance projection: slightly larger type, stronger line-height and contrast, a more disciplined optical grid, and less decorative glow. Do not abandon the reference language. This direction should answer whether faithful styling can remain unusually readable in a bright sanctuary.

### C — Warm sanctuary

Interpret the reference through the quiet typographic discipline of Studio Dumbar or Pentagram editorial work, but make it warmer and more devotional rather than more decorative. A restrained amber halo or subtle architectural-light motif is acceptable. Keep the same palette family and content hierarchy. This direction should feel like the reference after an art director gave it a little more atmosphere while preserving projection legibility.

## Acceptance criteria

Each direction must visibly include one song slide and one Bible slide, match the 16:9 ratio, use the exact Hungarian sample copy above, support accented characters, and preserve strong contrast. Typography must not overflow at the comparison screenshot size. The Bible layout must distinguish citation, verse number, and text. The song layout must distinguish section metadata from lyrics. The design must be achievable later with shared Svelte/CSS rendering rather than baked images. No production implementation starts until the user sees the three screenshots and selects a direction or asks for a blend.
