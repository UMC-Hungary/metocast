<script lang="ts">
  interface Props {
    /** Abbreviated month, already localised by the caller. */
    month: string;
    day: string | number;
    /**
     * The full date, announced in place of the two fragments. Without it a screen reader hears
     * "MAR 14" as two unrelated strings; the reference has no such affordance.
     */
    label?: string;
  }

  let { month, day, label }: Props = $props();
</script>

<div role={label ? 'img' : undefined} aria-label={label}>
  <span aria-hidden={label ? 'true' : undefined}>{month}</span>
  <strong aria-hidden={label ? 'true' : undefined}>{day}</strong>
</div>

<style>
  div {
    width: var(--c-date-block-width);
    text-align: center;
    border-right: var(--ui-border-hairline) solid var(--border-hairline);
    padding-right: var(--ui-gutter-inset);
    margin-right: var(--c-date-block-overlap);
    flex-shrink: 0;
  }

  span {
    display: block;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    color: var(--text-muted);
    letter-spacing: var(--type-label-sm-track);
  }

  strong {
    display: block;
    font-family: var(--type-numeral-family);
    font-size: var(--type-numeral-size);
    color: var(--text-primary);
    line-height: var(--type-numeral-leading);
    font-weight: var(--type-numeral-weight);
    margin-top: var(--c-date-block-gap);
  }
</style>
