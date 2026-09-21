<script lang="ts">
  interface Props {
    /** 0–100. Leave undefined for work whose extent is not known yet. */
    value?: number;
    label: string;
    /** The figure beside the label — "3 of 12", "48%". Text, so it can be translated. */
    valueText?: string;
  }

  let { value, label, valueText }: Props = $props();

  const clamped = $derived(value === undefined ? undefined : Math.min(100, Math.max(0, value)));
</script>

<div class="progress">
  <p>
    <span class="label">{label}</span>
    {#if valueText}<span class="value">{valueText}</span>{/if}
  </p>
  <div
    class="track"
    role="progressbar"
    aria-label={label}
    aria-valuenow={clamped}
    aria-valuemin={clamped === undefined ? undefined : 0}
    aria-valuemax={clamped === undefined ? undefined : 100}
    aria-valuetext={valueText}
  >
    <div
      class="fill"
      class:indeterminate={clamped === undefined}
      style:width={clamped === undefined ? undefined : `${clamped}%`}
    ></div>
  </div>
</div>

<style>
  p {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin: 0 0 var(--c-progress-label-gap);
  }

  .label,
  .value {
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--type-label-sm-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  .track {
    height: var(--c-progress-height);
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--surface-inverse);
    transition: width var(--motion-slide) var(--motion-ease-standard);
  }

  /* A fraction of the track, expressed as a ratio rather than a width — it is not a measurement. */
  .indeterminate {
    width: 100%;
    transform-origin: left;
    animation: sanctum-progress var(--motion-pulse) var(--motion-ease-default) infinite;
  }

  @keyframes sanctum-progress {
    from {
      transform: translateX(-100%) scaleX(0.4);
    }
    to {
      transform: translateX(250%) scaleX(0.4);
    }
  }
</style>
