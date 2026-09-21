<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { RefreshCw, RotateCcw, Trash2 } from 'lucide-svelte';
  import { toast } from 'svelte-sonner';
  import { _ } from 'svelte-i18n';
  import { listQueues, listJobs, retryJob, purgeJob } from '@metocast/core-client';
  import { queues } from '$lib/stores/queues.js';
  import type { Job } from '@metocast/core-client/schemas/queue';

  const statuses = ['all', 'pending', 'processing', 'succeeded', 'dead'] as const;
  type StatusFilter = (typeof statuses)[number];

  let selectedQueue = $state('platform_sync');
  let selectedStatus = $state<StatusFilter>('all');
  let jobs = $state<Job[]>([]);
  let isLoading = $state(false);
  let now = $state(Date.now());

  // Re-fetch the table whenever the live stats change (a job moved state).
  const unsubscribe = queues.subscribe(() => void loadJobs());
  const ticker = setInterval(() => (now = Date.now()), 1000);

  onMount(() => {
    void refresh();
  });

  onDestroy(() => {
    unsubscribe();
    clearInterval(ticker);
  });

  async function refresh(): Promise<void> {
    isLoading = true;
    try {
      queues.set(await listQueues());
      await loadJobs();
    } catch (e) {
      toast.error($_('queuesPage.loadFailed'), { description: errorText(e) });
    } finally {
      isLoading = false;
    }
  }

  async function loadJobs(): Promise<void> {
    try {
      jobs = await listJobs(selectedQueue, selectedStatus === 'all' ? undefined : selectedStatus);
    } catch (e) {
      toast.error($_('queuesPage.loadFailed'), { description: errorText(e) });
    }
  }

  async function onRetry(id: string): Promise<void> {
    try {
      await retryJob(id);
      toast.success($_('queuesPage.redriven'));
    } catch (e) {
      toast.error($_('queuesPage.actionFailed'), { description: errorText(e) });
    }
  }

  async function onPurge(id: string): Promise<void> {
    try {
      await purgeJob(id);
    } catch (e) {
      toast.error($_('queuesPage.actionFailed'), { description: errorText(e) });
    }
  }

  function selectStatus(status: StatusFilter): void {
    selectedStatus = status;
    void loadJobs();
  }

  /// Seconds until a pending job becomes due; null when it is due already.
  function countdown(job: Job): string | null {
    if (job.status !== 'pending') return null;
    const delta = Math.round((new Date(job.availableAt).getTime() - now) / 1000);
    return delta > 0 ? `${delta}s` : null;
  }

  function age(iso: string | null): string {
    if (!iso) return '—';
    const secs = Math.max(0, Math.round((now - new Date(iso).getTime()) / 1000));
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    return `${Math.floor(secs / 3600)}h`;
  }

  function summary(text: unknown): string {
    return JSON.stringify(text ?? {}).slice(0, 80);
  }

  function errorText(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
</script>

<svelte:head>
  <title>{$_('queuesPage.title')} — Sermon Helper</title>
</svelte:head>

<div class="queues-page">
  <div class="page-header">
    <h1>{$_('queuesPage.title')}</h1>
    <button class="icon-button" type="button" onclick={refresh} disabled={isLoading}>
      <RefreshCw size={16} />
      <span>{$_('queuesPage.refresh')}</span>
    </button>
  </div>

  <div class="cards">
    {#each $queues as q (q.queue)}
      <button
        class="card"
        class:active={q.queue === selectedQueue}
        type="button"
        onclick={() => {
          selectedQueue = q.queue;
          void loadJobs();
        }}
      >
        <span class="card-title">{q.queue}</span>
        <span class="metrics">
          <span>{$_('queuesPage.available')}: <b>{q.pending}</b></span>
          <span>{$_('queuesPage.inFlight')}: <b>{q.processing}</b></span>
          <span class:danger={q.dead > 0}>{$_('queuesPage.dlq')}: <b>{q.dead}</b></span>
          <span>{$_('queuesPage.headAge')}: <b>{age(q.oldestAvailableAt)}</b></span>
        </span>
      </button>
    {:else}
      <p class="empty">{$_('queuesPage.noQueues')}</p>
    {/each}
  </div>

  <div class="filter-row">
    {#each statuses as status (status)}
      <button
        type="button"
        class:active={selectedStatus === status}
        onclick={() => selectStatus(status)}
      >
        {$_(`queuesPage.filters.${status}`)}
      </button>
    {/each}
  </div>

  <div class="table-surface">
    <table>
      <thead>
        <tr>
          <th>{$_('queuesPage.columns.type')}</th>
          <th>{$_('queuesPage.columns.status')}</th>
          <th>{$_('queuesPage.columns.attempts')}</th>
          <th>{$_('queuesPage.columns.nextRetry')}</th>
          <th>{$_('queuesPage.columns.payload')}</th>
          <th>{$_('queuesPage.columns.lastError')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each jobs as job (job.id)}
          <tr>
            <td>{job.jobType}</td>
            <td><span class="badge status-{job.status}">{job.status}</span></td>
            <td>{job.attempts}/{job.maxAttempts}</td>
            <td>{countdown(job) ?? '—'}</td>
            <td class="mono">{summary(job.payload)}</td>
            <td class="mono error">{job.lastError ?? ''}</td>
            <td class="actions">
              <button
                type="button"
                title={$_('queuesPage.redrive')}
                onclick={() => onRetry(job.id)}
              >
                <RotateCcw size={14} />
              </button>
              <button type="button" title={$_('queuesPage.purge')} onclick={() => onPurge(job.id)}>
                <Trash2 size={14} />
              </button>
            </td>
          </tr>
        {:else}
          <tr><td colspan="7" class="empty">{$_('queuesPage.noJobs')}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .queues-page {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
  }

  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .icon-button {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 2rem;
    padding: 0 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.8125rem;
    cursor: pointer;
  }

  .cards {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 18rem;
    padding: 0.875rem 1rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--glass-card-bg);
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
  }

  .card.active {
    border-color: var(--nav-item-active-bg);
  }

  .card-title {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .metrics {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    color: var(--text-secondary);
    font-size: 0.78rem;
  }

  .metrics .danger {
    color: var(--status-err-text);
  }

  .filter-row {
    display: flex;
    gap: 0.25rem;
  }

  .filter-row button {
    min-height: 2rem;
    padding: 0 0.75rem;
    border: none;
    border-radius: 0.375rem;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
  }

  .filter-row button.active {
    background: var(--nav-item-active-bg);
    color: var(--nav-item-active-text);
  }

  .table-surface {
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    background: var(--glass-card-bg);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th,
  td {
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid var(--border);
    text-align: left;
    vertical-align: top;
  }

  th {
    color: var(--text-secondary);
    font-size: 0.72rem;
    text-transform: uppercase;
  }

  .mono {
    max-width: 22rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    overflow-wrap: anywhere;
  }

  .error {
    color: var(--status-err-text);
  }

  .badge {
    padding: 0.1rem 0.4rem;
    border-radius: 9999px;
    background: var(--content-bg);
    font-size: 0.72rem;
  }

  .badge.status-dead {
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }

  .badge.status-processing {
    background: var(--status-warn-bg);
    color: var(--status-warn-text);
  }

  .actions {
    display: flex;
    gap: 0.25rem;
  }

  .actions button {
    display: inline-flex;
    padding: 0.25rem;
    border: none;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .empty {
    color: var(--text-secondary);
    font-size: 0.8125rem;
  }
</style>
