<script lang="ts" module>
  export interface PresenterClient {
    id: string;
    name: string;
    /** The address or identifier, set in the mono face. */
    address: string;
    /** How long it has been connected, or its latency. */
    detail?: string;
  }
</script>

<script lang="ts">
  interface Props {
    /** The address a presenter opens to join. */
    url: string;
    copyLabel: string;
    label: string;
    clientsLabel: string;
    /** "3 connected", already interpolated. */
    clientsSummary?: string;
    clients: PresenterClient[];
    emptyMessage: string;
    oncopy?: () => void;
  }

  let {
    url,
    copyLabel,
    label,
    clientsLabel,
    clientsSummary = '',
    clients,
    emptyMessage,
    oncopy,
  }: Props = $props();
</script>

<section class="support" aria-label={label}>
  <div class="url">
    <span>{url}</span>
    <button type="button" onclick={oncopy}>{copyLabel}</button>
  </div>

  <div class="clients">
    <header>
      <span>{clientsLabel}</span>
      {#if clientsSummary}<span>{clientsSummary}</span>{/if}
    </header>
    {#if clients.length === 0}
      <p class="empty">{emptyMessage}</p>
    {:else}
      <ul>
        {#each clients as client (client.id)}
          <li>
            <strong>{client.name}</strong>
            <span>{client.address}</span>
            {#if client.detail}<em>{client.detail}</em>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .support {
    display: grid;
    gap: var(--ui-stack-loose);
  }

  .url {
    display: flex;
    border: var(--ui-border-hairline) solid var(--border-control);
    background: var(--surface-raised);
  }

  .url span {
    flex: 1;
    min-width: 0;
    padding: var(--c-presenter-url-padding-block) var(--ui-gutter-tight);
    color: var(--text-primary);
    font-family: var(--type-label-family);
    font-size: var(--c-presenter-url-size);
    word-break: break-all;
  }

  .url button {
    min-height: var(--ui-target-min);
    padding: 0 var(--ui-gutter-inset);
    border: 0;
    background: var(--surface-inverse);
    color: var(--text-inverse);
    cursor: pointer;
    white-space: nowrap;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-presenter-button-track);
    text-transform: var(--type-label-sm-transform);
  }

  .url button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .clients {
    background: var(--surface-raised);
    border-top: var(--ui-border-hairline) solid var(--border-hairline);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  .clients header,
  li {
    padding: var(--c-presenter-row-padding-block) var(--ui-gutter-inset);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  .clients header {
    display: flex;
    justify-content: space-between;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-presenter-header-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li:last-child {
    border-bottom: 0;
  }

  strong,
  li span,
  em {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  strong {
    font-family: var(--type-body-sm-family);
    font-size: var(--type-body-sm-size);
    color: var(--text-primary);
  }

  li span,
  em {
    margin-top: var(--c-toggle-row-sub-gap);
    color: var(--text-muted);
    font-family: var(--type-label-family);
    font-size: var(--c-presenter-detail-size);
    font-style: normal;
  }

  .empty {
    margin: 0;
    padding: var(--c-presenter-row-padding-block) var(--ui-gutter-inset);
    font-family: var(--type-quote-family);
    font-style: italic;
    font-size: var(--type-quote-size);
    line-height: var(--type-body-sm-leading);
    color: var(--text-muted);
  }
</style>
