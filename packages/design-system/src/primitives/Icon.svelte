<script lang="ts" module>
  /**
   * The reference's own 16-glyph set, plus the marks the components it never had require.
   * All are 24×24 stroked outlines so a size change never changes the drawing's weight.
   */
  export const icons = {
    home: 'M3 11l9-8 9 8v10a1 1 0 0 1-1 1h-5v-7h-6v7H4a1 1 0 0 1-1-1V11z',
    calendar:
      'M4.5 5.75h15a1.5 1.5 0 0 1 1.5 1.5v12.25a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 19.5V7.25a1.5 1.5 0 0 1 1.5-1.5zM3 10h18M8 3v4M16 3v4',
    clock: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3.5 2',
    slides:
      'M4 5h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM12 17v3M9 20h6',
    gear: 'M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z',
    chev: 'M9 6l6 6-6 6',
    back: 'M15 6l-6 6 6 6',
    down: 'M6 9l6 6 6-6',
    up: 'M6 15l6-6 6 6',
    plus: 'M12 5v14M5 12h14',
    close: 'M6 6l12 12M18 6L6 18',
    check: 'M5 12.5l4.5 4.5L19 7',
    minus: 'M6 12h12',
    search: 'M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm9 16-3.5-3.5',
    first: 'M6 5v14M18 6l-8 6 8 6V6z',
    prev: 'M16 6l-8 6 8 6V6z',
    next: 'M8 6l8 6-8 6V6z',
    last: 'M18 5v14M6 6l8 6-8 6V6z',
    play: 'M8 5l11 7-11 7V5z',
    stop: 'M8 7h8a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1z',
    record: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z',
    stream:
      'M12 13.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3M8.6 15.4a4.8 4.8 0 0 1 0-6.8M15.4 8.6a4.8 4.8 0 0 1 0 6.8M5.8 18.2a9 9 0 0 1 0-12.4M18.2 5.8a9 9 0 0 1 0 12.4',
    card: 'M6 3h9l4 4v14a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM9 3v5h6',
    bell: 'M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9M13.73 21a2 2 0 0 1-3.46 0',
    alert: 'M12 3.5 22 20H2L12 3.5zM12 10v4.5M12 17.5h.01',
    info: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 8h.01M11 12h1v5h1',
    retry: 'M20 12a8 8 0 1 1-2.34-5.66M20 4v4h-4',
  } as const;

  export type IconName = keyof typeof icons;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    /** Matches the reference's per-call sizing; the drawing scales, the stroke does not. */
    size?: number;
    stroke?: number;
    /** Decorative by default. Give a label only when the icon is the sole carrier of meaning. */
    label?: string;
  }

  let { name, size = 20, stroke = 1.4, label }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  style:--sanctum-icon-stroke={stroke}
  role={label ? 'img' : 'presentation'}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
  focusable="false"
>
  {#if label}<title>{label}</title>{/if}
  <path d={icons[name]} />
</svg>

<style>
  svg {
    display: block;
    fill: none;
    stroke: currentColor;
    stroke-width: var(--sanctum-icon-stroke);
    stroke-linecap: round;
    stroke-linejoin: round;
    flex-shrink: 0;
  }
</style>
