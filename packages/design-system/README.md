# Sanctum design system

Tokens and accessible components for the Sanctum UI. Every value in it is measured from the
design reference at `~/workspace/ui/sermon-helper-svelte` rather than fitted to a scale, and
every measured value is re-checked mechanically on each build.

Nothing consumes this package yet. It is a prerequisite for `split-into-two-uis`, and it is meant
to be finished, reviewed and signed off before any UI depends on it.

## Using it

```js
import '@metocast/design-system/fonts.css';
import '@metocast/design-system/tokens.css';
import '@metocast/design-system/base.css';

import { Row, List, SectionLabel } from '@metocast/design-system';
```

Set the colour scheme with `data-scheme="light"` or `data-scheme="dark"` on the document root.
Without the attribute the system preference decides. It can be changed at any time without a
reload or a remount.

## How the values got here

```
scripts/extract-reference.mjs   parses the reference's stylesheets
        ↓
tokens/measurements.json        one row per declaration, with its breakpoint context
tokens/measurements.md          the same record, readable and committed
        ↓
tokens/tokens.json              the scale, named after the values it holds
tokens/themes/sanctum.json      both colour schemes
        ↓
tokens/generated/tokens.css     committed, so no build step is needed to use the package
```

Primitives are named after their measured value — `--space-14`, `--text-15`, `--leading-1-24` —
so there is no ordinal ladder for a component to round along. The scale looks irregular because
the design is irregular: it is built on 14, 24, 56, 34 and 10-at-2px-tracking, and a 4px grid
cannot express most of that. That is the point, and it should not be tidied up.

Components consume semantic (`--ui-*`, `--type-*`, `--motion-*`) or component (`--c-*`) tokens
only. A literal value in a component's styles is a build failure.

## Checks

| Command                               | What it holds to                                                                                                                                                                                                                            |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm measure`                        | Regenerates the measurement record from the reference                                                                                                                                                                                       |
| `pnpm measure:check`                  | The committed record matches the reference                                                                                                                                                                                                  |
| `pnpm tokens:build`                   | Regenerates `tokens.css`                                                                                                                                                                                                                    |
| `pnpm tokens:check`                   | `tokens.css` matches its source; both schemes are complete; every value is measured or carries a recorded deviation; the accent is ≥60° from the status colours and stays distinguishable under the three common colour-vision deficiencies |
| `pnpm contrast`                       | Every documented pairing meets WCAG 2.2 AA in both schemes                                                                                                                                                                                  |
| `pnpm fidelity`                       | Every component token still equals the declaration it was taken from; no literals; no undeclared breakpoints                                                                                                                                |
| `pnpm docs` / `pnpm docs:check`       | One specification per component, generated from the source so it cannot drift                                                                                                                                                               |
| `pnpm stories` / `pnpm stories:check` | One story file per component                                                                                                                                                                                                                |
| `pnpm catalog:check`                  | Every export has a specification and stories, and nothing documents something unexported                                                                                                                                                    |
| `pnpm verify`                         | All of the above                                                                                                                                                                                                                            |
| `pnpm test:a11y`                      | Accessibility checks over every story, in a real browser                                                                                                                                                                                    |
| `pnpm storybook`                      | The catalog                                                                                                                                                                                                                                 |

`pnpm fonts` re-downloads the woff2 subsets. They are committed, so it is only needed when the
type roles change.

## Where things live

```
tokens/       measurements, the scale, both schemes, the fidelity map, generated CSS
src/
  primitives/ the smallest components
  patterns/   compositions
  a11y/       focus-trap, roving-tabindex, dismissable, live-region — carried over unchanged
  index.ts    the only public entry point
docs/         one specification per component; specs.json holds the hand-authored parts
stories/      one story file per component; samples.json holds the sample content
static/fonts/ self-hosted woff2 subsets, latin and latin-ext
.storybook/   catalog configuration
```

Storybook is a development dependency of this package alone. No consuming application carries it.

## Deviating from the reference

A component may depart from the reference, but never quietly. A departure belongs in
`tokens/fidelity.json` under `deviations` when it is a token value, or in `docs/specs.json` under
`deviations` when it is behaviour — and it needs a reason. Every deviation currently recorded is
either a contrast correction or an accessibility behaviour the reference does not have. "The grid
said so" is not a reason; that is what produced the version of this system that was thrown away.
