<script lang="ts">
  import { listAllRecordings } from '@metocast/core-client';
  import { lastWsMessage } from '$lib/stores/ws.js';
  import { uploadProgress } from '$lib/stores/uploads.js';
  import type { RecordingWithEvent } from '@metocast/core-client/schemas/recording';
  import type { UploadProgressEntry } from '$lib/stores/uploads.js';

  type FilterTab = 'not_flagged' | 'flagged' | 'in_progress' | 'uploaded';

  let activeTab = $state<FilterTab>('in_progress');
  let items = $state<RecordingWithEvent[]>([]);
  let loading = $state(false);

  async function loadTab(filter: FilterTab) {
    loading = true;
    try {
      items = await listAllRecordings(filter);
    } catch {
      items = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    loadTab(activeTab);
  });

  $effect(() => {
    const msg = $lastWsMessage;
    if (!msg) return;
    if (
      msg.type === 'upload.progress' ||
      msg.type === 'upload.completed' ||
      msg.type === 'upload.failed' ||
      msg.type === 'upload.paused'
    ) {
      if (activeTab === 'in_progress') {
        loadTab('in_progress');
      }
    } else if (msg.type === 'recording.changed') {
      loadTab(activeTab);
    }
  });

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatDuration(seconds: number): string {
    if (seconds <= 0) return '';
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  function getLiveUploads(rec: RecordingWithEvent): UploadProgressEntry[] {
    return Object.entries($uploadProgress)
      .filter(([k]) => k.startsWith(`${rec.id}:`))
      .map(([, v]) => v);
  }
</script>

<div class="recordings-block">
  <div class="tabs" role="tablist">
    <button
      role="tab"
      aria-selected={activeTab === 'not_flagged'}
      class="tab"
      class:tab--active={activeTab === 'not_flagged'}
      onclick={() => (activeTab = 'not_flagged')}>Not Flagged</button
    >
    <button
      role="tab"
      aria-selected={activeTab === 'flagged'}
      class="tab"
      class:tab--active={activeTab === 'flagged'}
      onclick={() => (activeTab = 'flagged')}>Flagged</button
    >
    <button
      role="tab"
      aria-selected={activeTab === 'in_progress'}
      class="tab"
      class:tab--active={activeTab === 'in_progress'}
      onclick={() => (activeTab = 'in_progress')}>In Progress</button
    >
    <button
      role="tab"
      aria-selected={activeTab === 'uploaded'}
      class="tab"
      class:tab--active={activeTab === 'uploaded'}
      onclick={() => (activeTab = 'uploaded')}>Uploaded</button
    >
  </div>

  <div class="tab-panel" role="tabpanel">
    {#if loading}
      <p class="empty">Loading…</p>
    {:else if items.length === 0}
      <p class="empty">No recordings.</p>
    {:else}
      <ul class="list">
        {#each items as rec (rec.id)}
          <li class="item">
            <div class="item__info">
              <span class="item__name">{rec.customTitle ?? rec.fileName}</span>
              <span class="item__meta">
                <a href="/events/{rec.eventId}" class="event-link">{rec.eventTitle}</a>
                {#if rec.fileSize > 0}
                  &nbsp;·&nbsp;{formatSize(rec.fileSize)}
                {/if}
                {#if formatDuration(rec.durationSeconds)}
                  &nbsp;·&nbsp;{formatDuration(rec.durationSeconds)}
                {/if}
              </span>
            </div>

            <div class="item__status">
              {#if activeTab === 'not_flagged'}
                <span class="badge badge--muted">Not queued</span>
              {:else if activeTab === 'flagged'}
                <span class="badge badge--warn">Pending</span>
              {:else if activeTab === 'uploaded'}
                {#if rec.videoUrl}
                  <a
                    href={rec.videoUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    class="badge badge--ok">Watch</a
                  >
                {:else}
                  <span class="badge badge--ok">Uploaded</span>
                {/if}
              {:else}
                {@const liveUploads = getLiveUploads(rec)}
                {#if liveUploads.length > 0}
                  {#each liveUploads as entry (entry.platform)}
                    <div class="upload-row">
                      <span class="platform-label">{entry.platform}</span>
                      {#if entry.state === 'uploading'}
                        {@const pct =
                          entry.totalBytes > 0
                            ? Math.round((entry.progressBytes / entry.totalBytes) * 100)
                            : 0}
                        <div
                          class="progress-bar"
                          role="progressbar"
                          aria-valuenow={pct}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-label="{entry.platform} upload progress"
                        >
                          <div class="progress-bar__fill" style="width:{pct}%"></div>
                        </div>
                        <span class="pct-label">{pct}%</span>
                      {:else if entry.state === 'failed'}
                        <span class="status-text status-text--err">{entry.error ?? 'Failed'}</span>
                      {:else if entry.state === 'paused'}
                        <span class="status-text status-text--warn">Paused</span>
                      {:else if entry.state === 'completed'}
                        <span class="status-text status-text--ok">Done</span>
                      {/if}
                    </div>
                  {/each}
                {:else}
                  {#each rec.uploads as upload (upload.platform)}
                    <div class="upload-row">
                      <span class="platform-label">{upload.platform}</span>
                      {#if upload.state === 'uploading'}
                        {@const pct =
                          upload.totalBytes > 0
                            ? Math.round((upload.progressBytes / upload.totalBytes) * 100)
                            : 0}
                        <div
                          class="progress-bar"
                          role="progressbar"
                          aria-valuenow={pct}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-label="{upload.platform} upload progress"
                        >
                          <div class="progress-bar__fill" style="width:{pct}%"></div>
                        </div>
                        <span class="pct-label">{pct}%</span>
                      {:else if upload.state === 'failed'}
                        <span class="status-text status-text--err">{upload.error ?? 'Failed'}</span>
                      {:else if upload.state === 'paused'}
                        <span class="status-text status-text--warn">Paused</span>
                      {:else if upload.state === 'completed'}
                        <span class="status-text status-text--ok">Done</span>
                      {:else}
                        <span class="status-text">{upload.state}</span>
                      {/if}
                    </div>
                  {/each}
                {/if}
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .recordings-block {
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    overflow: hidden;
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
    background: var(--nav-item-hover, rgba(0, 0, 0, 0.03));
  }

  .tab {
    padding: 0.5rem 1rem;
    font-size: 0.875rem;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-secondary);
    cursor: pointer;
    margin-bottom: -1px;
  }

  .tab:hover {
    color: var(--text-primary);
  }

  .tab--active {
    color: var(--accent);
    border-bottom-color: var(--accent);
    font-weight: 500;
  }

  .tab-panel {
    padding: 0.75rem;
  }

  .empty {
    color: var(--text-secondary);
    font-size: 0.875rem;
    margin: 0;
    padding: 0.5rem 0;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .item {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.625rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    flex-wrap: wrap;
  }

  .item__info {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .item__name {
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item__meta {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .event-link {
    color: var(--accent);
    text-decoration: none;
  }

  .event-link:hover {
    text-decoration: underline;
  }

  .item__status {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    align-items: flex-end;
    flex-shrink: 0;
  }

  .badge {
    font-size: 0.75rem;
    font-weight: 500;
    padding: 0.125rem 0.5rem;
    border-radius: 0.25rem;
  }

  .badge--muted {
    color: var(--text-secondary);
    background: var(--nav-item-hover, rgba(0, 0, 0, 0.05));
  }

  .badge--warn {
    color: var(--status-warn-text);
    background: var(--status-warn-bg);
  }

  .badge--ok {
    color: var(--status-ok-text);
    background: var(--status-ok-bg);
    text-decoration: none;
  }

  a.badge--ok:hover {
    filter: brightness(0.9);
  }

  .upload-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .platform-label {
    font-size: 0.75rem;
    color: var(--text-secondary);
    text-transform: capitalize;
    min-width: 4rem;
  }

  .progress-bar {
    width: 8rem;
    height: 0.375rem;
    background: var(--border);
    border-radius: 9999px;
    overflow: hidden;
  }

  .progress-bar__fill {
    height: 100%;
    background: var(--accent);
    border-radius: 9999px;
    transition: width 0.2s ease;
  }

  .pct-label {
    font-size: 0.75rem;
    color: var(--text-secondary);
    min-width: 2.5rem;
    text-align: right;
  }

  .status-text {
    font-size: 0.75rem;
  }

  .status-text--ok {
    color: var(--status-ok-text);
  }
  .status-text--warn {
    color: var(--status-warn-text);
  }
  .status-text--err {
    color: var(--status-err-text);
  }
</style>
