<script lang="ts">
  interface Props {
    label: string;
    value: string | number;
    /** A short unit set beside the figure — "min", "kb/s". */
    unit?: string;
  }

  let { label, value, unit = '' }: Props = $props();
</script>

<div class="stat">
  <div class="label">{label}</div>
  <p class="value">
    <span>{value}</span>
    {#if unit}<small>{unit}</small>{/if}
  </p>
</div>

<style>
  .label {
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-stat-label-track);
    text-transform: var(--type-label-transform);
    color: var(--text-muted);
    margin-bottom: var(--c-stat-label-gap);
  }

  .value {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: var(--c-stat-unit-gap);
  }

  span {
    font-family: var(--type-figure-family);
    font-size: var(--type-figure-size);
    font-weight: var(--type-figure-weight);
    color: var(--text-primary);
    letter-spacing: var(--type-figure-track);
    line-height: var(--type-figure-leading);
  }

  small {
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    color: var(--text-muted);
  }
</style>
