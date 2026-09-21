# Component name

One sentence on what the component is for, and what it is not for.

## Anatomy

The parts, named. A part that has no name here has no business having a style.

## Props

| Prop | Type | Default | Meaning |
| ---- | ---- | ------- | ------- |

## Variants

| Variant | When to use |
| ------- | ----------- |

## States

Default, hover, focus-visible, active, disabled, loading, error, empty — whichever apply. A state
listed here must have a story.

## Tokens consumed

| Token | Used for |
| ----- | -------- |

Components consume semantic (`--ui-*`, `--type-*`, `--motion-*`) or component (`--c-*`) tokens
only. A literal value in a component's styles is a defect.

## Keyboard

| Key | Behaviour |
| --- | --------- |

Non-interactive components state "Not interactive" and nothing else.

## ARIA

Roles, properties and states, and which WAI-ARIA Authoring Practices pattern they come from.

## Accessibility acceptance criteria

- Reachable, operable and leavable by keyboard alone, with a visible focus indicator at 3:1.
- Accessible name present for every control.
- No meaning carried by colour alone.
- Legible and operable at 360px and at desktop width, at 200% zoom.
- Touch target at least 44 × 44px on touch-capable viewports.

## Reference correspondence

| Property | Reference | Implemented |
| -------- | --------- | ----------- |

Source: `src/components/…` in the design reference. `None` for a component the reference lacks,
with a note on which reference components its idiom is taken from.

## Recorded deviations

| What | Why |
| ---- | --- |

A deviation without a reason is a defect. "The grid said so" is not a reason.
