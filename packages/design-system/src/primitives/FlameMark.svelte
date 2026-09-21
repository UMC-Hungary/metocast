<script lang="ts">
  interface Props {
    size?: number;
    /** Names the mark when it stands alone. Inside `Lockup` the wordmark names it instead. */
    label?: string;
  }

  let { size = 22, label }: Props = $props();

  // A gradient needs a document-unique id, and the mark may appear more than once on a page.
  const gradientId = `sanctum-ember-${crypto.randomUUID()}`;
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 160 160"
  role={label ? 'img' : 'presentation'}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
  focusable="false"
>
  {#if label}<title>{label}</title>{/if}
  <defs>
    <linearGradient id={gradientId} x1="80" y1="22" x2="80" y2="130" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#FDE7C1" />
      <stop offset="0.45" stop-color="#E89A54" />
      <stop offset="1" stop-color="#B8331C" />
    </linearGradient>
  </defs>
  <path
    d="M 80 22 C 100 44, 116 64, 116 88 C 116 110, 102 126, 80 130 C 58 126, 44 110, 44 88 C 44 66, 62 46, 80 22 Z"
    fill={`url(#${gradientId})`}
  />
  <path
    d="M 80 58 C 92 72, 98 84, 98 96 C 98 108, 90 116, 80 118 C 70 116, 62 108, 62 96 C 62 84, 70 74, 80 58 Z"
    fill="#FDE7C1"
  />
</svg>

<style>
  svg {
    display: block;
    flex-shrink: 0;
  }
</style>
