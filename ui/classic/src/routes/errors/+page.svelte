<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { connectObs, fetchConnectorStatuses } from '@metocast/core-client';
  import { applyConnectorStatuses } from '$lib/stores/connectors.js';
  import { connectorErrors, clearError } from '$lib/stores/errors.js';
  import { findConnector } from '$lib/connectors/registry.js';
  import ConnectorFixModal from '$lib/components/connectors/ConnectorFixModal.svelte';

  let fixingConnectorId: string | null = $state(null);
  let recheckingIds = $state(new Set<string>());
  let expandedInfoIds = $state(new Set<string>());

  async function recheck(connectorId: string, errorId: string) {
    recheckingIds = new Set([...recheckingIds, errorId]);
    clearError(errorId);
    try {
      // OBS is the only connector with a manual reconnect; the rest simply
      // report their current state.
      if (connectorId === 'obs') await connectObs();
      applyConnectorStatuses(await fetchConnectorStatuses());
    } catch {
      // A failed re-check leaves the existing error in place; status updates
      // also arrive over WS.
    } finally {
      recheckingIds = new Set([...recheckingIds].filter((id) => id !== errorId));
    }
  }

  function toggleInfo(errorId: string) {
    if (expandedInfoIds.has(errorId)) {
      expandedInfoIds = new Set([...expandedInfoIds].filter((id) => id !== errorId));
    } else {
      expandedInfoIds = new Set([...expandedInfoIds, errorId]);
    }
  }

  type MarkdownPart = { text: string; bold: boolean };
  type MarkdownBlock = { kind: 'h2' | 'h3' | 'li' | 'p' | 'br'; parts: MarkdownPart[] };

  /** The infoMarkdown field as blocks the template renders, so no HTML string is ever built. */
  function parseMarkdown(md: string): MarkdownBlock[] {
    return md.split('\n').map((line): MarkdownBlock => {
      if (line.startsWith('## '))
        return { kind: 'h3', parts: [{ text: line.slice(3), bold: false }] };
      if (line.startsWith('# '))
        return { kind: 'h2', parts: [{ text: line.slice(2), bold: false }] };
      if (line.startsWith('- ') || /^\d+\. /.test(line)) {
        return { kind: 'li', parts: boldParts(line.replace(/^[-\d]+[.) ] */, '')) };
      }
      if (line.trim() === '') return { kind: 'br', parts: [] };
      return { kind: 'p', parts: boldParts(line) };
    });
  }

  /** `**bold**` runs: splitting on a capture group puts the bold text at the odd positions. */
  function boldParts(text: string): MarkdownPart[] {
    return text
      .split(/\*\*(.+?)\*\*/)
      .map((chunk, i) => ({ text: chunk, bold: i % 2 === 1 }))
      .filter((part) => part.text !== '');
  }
</script>

<svelte:head>
  <title>{$_('errorsPage.title')} — Metocast</title>
</svelte:head>

{#snippet inline(parts: MarkdownPart[])}
  {#each parts as part, i (i)}{#if part.bold}<strong>{part.text}</strong
      >{:else}{part.text}{/if}{/each}
{/snippet}

<h1>{$_('errorsPage.title')}</h1>

{#if $connectorErrors.length === 0}
  <p class="no-errors">{$_('errorsPage.noErrors')}</p>
{:else}
  <div class="error-list">
    {#each $connectorErrors as err (err.id)}
      {@const def = findConnector(err.connectorId)}
      <div class="error-card">
        <div class="error-header">
          <div class="error-title">
            <strong>{err.connectorName}</strong>
            <span class="error-message">{err.message}</span>
          </div>
          <div class="error-actions">
            <button
              class="btn-secondary btn-sm"
              onclick={() => recheck(err.connectorId, err.id)}
              disabled={recheckingIds.has(err.id)}
            >
              {recheckingIds.has(err.id) ? $_('errorsPage.rechecking') : $_('errorsPage.recheck')}
            </button>
            <button
              class="btn-primary btn-sm"
              onclick={() => {
                fixingConnectorId = err.connectorId;
              }}
            >
              {$_('errorsPage.fix')}
            </button>
            {#if def?.infoMarkdown}
              <button class="btn-info btn-sm" onclick={() => toggleInfo(err.id)}>
                {expandedInfoIds.has(err.id) ? $_('errorsPage.closeInfo') : $_('errorsPage.info')}
              </button>
            {/if}
          </div>
        </div>

        {#if def?.infoMarkdown && expandedInfoIds.has(err.id)}
          <div class="info-panel">
            {#each parseMarkdown(def.infoMarkdown) as block, i (i)}
              {#if block.kind === 'br'}
                <br />
              {:else if block.kind === 'h2'}
                <h2>{@render inline(block.parts)}</h2>
              {:else if block.kind === 'h3'}
                <h3>{@render inline(block.parts)}</h3>
              {:else if block.kind === 'li'}
                <li>{@render inline(block.parts)}</li>
              {:else}
                <p>{@render inline(block.parts)}</p>
              {/if}
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

{#if fixingConnectorId !== null}
  <ConnectorFixModal
    connectorId={fixingConnectorId}
    onClose={() => {
      fixingConnectorId = null;
    }}
  />
{/if}

<style>
  h1 {
    margin-bottom: 1.5rem;
  }

  .no-errors {
    color: var(--text-secondary);
    font-size: 0.9375rem;
  }

  .error-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    max-width: 700px;
  }

  .error-card {
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    padding: 1rem;
    background: var(--glass-card-bg);
  }

  .error-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: wrap;
  }

  .error-title {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .error-message {
    font-size: 0.875rem;
    color: var(--text-secondary);
  }

  .error-actions {
    display: flex;
    gap: 0.5rem;
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .info-panel {
    margin-top: 1rem;
    padding: 0.75rem;
    background: var(--content-bg);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    line-height: 1.6;
  }

  :global(.info-panel h2),
  :global(.info-panel h3) {
    margin: 0.5rem 0 0.25rem;
    font-size: 0.9375rem;
  }

  :global(.info-panel p) {
    margin: 0.25rem 0;
  }

  :global(.info-panel li) {
    margin-left: 1.25rem;
    list-style: disc;
  }

  .btn-primary {
    padding: 0.375rem 0.75rem;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.8125rem;
    cursor: pointer;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-secondary {
    padding: 0.375rem 0.75rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    font-size: 0.8125rem;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--accent-subtle);
  }

  .btn-info {
    padding: 0.375rem 0.75rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.8125rem;
    cursor: pointer;
  }

  .btn-info:hover {
    background: var(--nav-item-hover);
  }

  .btn-sm {
    padding: 0.25rem 0.625rem;
    font-size: 0.8125rem;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
