<script lang="ts">
  interface Props {
    /** A single typographic character standing in for an icon, set in the display face. */
    char?: string;
    /** Give a name only when the character is the sole carrier of meaning. */
    label?: string;
  }

  let { char = '·', label }: Props = $props();
</script>

<span role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : 'true'}>
  {char}
</span>

<style>
  span {
    width: var(--c-text-icon-size);
    height: var(--c-text-icon-size);
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--type-heading-family);
    font-size: var(--c-text-icon-font-size);
    color: var(--text-primary);
    flex-shrink: 0;
  }
</style>
