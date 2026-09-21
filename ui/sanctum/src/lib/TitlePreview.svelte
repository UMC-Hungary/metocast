<script lang="ts">
  /** The published-title preview: an inverted band with the rendered title and,
   *  optionally, which fields are feeding it. Shared by the event editor and the
   *  template setting so the two cannot drift apart. */
  interface Props {
    label: string;
    text: string;
    count: string;
    warn?: boolean;
    tags?: { key: string; label: string; on: boolean }[];
  }

  let { label, text, count, warn = false, tags }: Props = $props();
</script>

<section class="preview">
  <small>{label} <span class:warn>{count}</span></small>
  <p>{text}</p>
  {#if tags}
    <div class="tags">
      {#each tags as t (t.key)}<em class:on={t.on}>{t.label}</em>{/each}
    </div>
  {/if}
</section>

<style>
  .preview {
    padding-block: 14px;
    padding-inline: 16px;
    background: var(--text-primary);
    color: var(--surface-base, var(--surface-outside));
  }
  .preview small {
    color: color-mix(in srgb, currentColor 55%, transparent);
    text-transform: uppercase;
    text-align: left;
    margin: 0 0 8px;
    display: flex;
    justify-content: space-between;
  }
  .preview span.warn {
    color: var(--status-warn);
  }
  .preview p {
    margin: 0;
    font-family: var(--font-display);
    font-size: 18px;
    line-height: 1.3;
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .tags {
    display: flex;
    gap: 14px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid color-mix(in srgb, currentColor 14%, transparent);
  }
  .preview em {
    font-family: var(--font-mono);
    font-size: 9px;
    color: color-mix(in srgb, currentColor 40%, transparent);
    letter-spacing: 1.3px;
    text-transform: uppercase;
    font-style: normal;
  }
  .preview em.on {
    color: currentColor;
  }
</style>
