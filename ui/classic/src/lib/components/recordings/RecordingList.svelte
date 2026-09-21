<script lang="ts">
  import { uploadProgress, type UploadProgressEntry } from '$lib/stores/uploads.js';
  import type { RecordingUpload } from '@metocast/core-client/schemas/recording';

  interface RecordingItem {
    id: string;
    fileName: string;
    filePath?: string;
    fileSize: number;
    durationSeconds: number;
    customTitle?: string | null | undefined;
    uploaded?: boolean;
    videoUrl?: string | null | undefined;
    detectedAt?: string;
    uploads?: RecordingUpload[];
  }

  interface Props {
    recordings: RecordingItem[];
    loading?: boolean;
    ondelete?: (id: string, deleteFile: boolean) => Promise<void>;
    onassign?: (id: string) => void;
  }

  let { recordings, loading = false, ondelete, onassign }: Props = $props();

  function getUploadBadge(
    rec: RecordingItem,
  ): { label: string; variant: 'ok' | 'warn' | 'err' | 'info' | 'none' } | null {
    // Check live WS progress first
    const liveKeys = Object.keys($uploadProgress).filter((k) => k.startsWith(`${rec.id}:`));
    if (liveKeys.length > 0) {
      // Combine statuses from all platforms
      const entries = liveKeys
        .map((k) => $uploadProgress[k])
        .filter((e): e is UploadProgressEntry => e !== undefined);
      const uploading = entries.find((e) => e.state === 'uploading');
      if (uploading) {
        const pct =
          uploading.totalBytes > 0
            ? Math.round((uploading.progressBytes / uploading.totalBytes) * 100)
            : 0;
        return { label: `Uploading ${pct}%`, variant: 'info' };
      }
      if (entries.some((e) => e.state === 'failed')) return { label: 'Failed', variant: 'err' };
      if (entries.some((e) => e.state === 'paused')) return { label: 'Paused', variant: 'warn' };
      if (entries.every((e) => e.state === 'completed'))
        return { label: 'Uploaded', variant: 'ok' };
    }

    // Fall back to uploads field from API
    const uploads = rec.uploads ?? [];
    if (uploads.length === 0) return null;
    if (uploads.some((u) => u.state === 'uploading')) {
      const u = uploads.find((u) => u.state === 'uploading')!;
      const pct = u.totalBytes > 0 ? Math.round((u.progressBytes / u.totalBytes) * 100) : 0;
      return { label: `Uploading ${pct}%`, variant: 'info' };
    }
    if (uploads.some((u) => u.state === 'failed'))
      return { label: 'Upload Failed', variant: 'err' };
    if (uploads.some((u) => u.state === 'paused'))
      return { label: 'Upload Paused', variant: 'warn' };
    if (uploads.some((u) => u.state === 'pending'))
      return { label: 'Upload Pending', variant: 'info' };
    if (uploads.every((u) => u.state === 'completed')) return { label: 'Uploaded', variant: 'ok' };
    return null;
  }

  let confirmingDeleteId = $state<string | null>(null);
  let deletingId = $state<string | null>(null);

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

  async function handleDelete(id: string, deleteFile: boolean) {
    deletingId = id;
    try {
      await ondelete?.(id, deleteFile);
    } finally {
      deletingId = null;
      confirmingDeleteId = null;
    }
  }
</script>

{#if loading}
  <p>Loading recordings…</p>
{:else if recordings.length === 0}
  <p class="empty">No recordings.</p>
{:else}
  <ul class="list">
    {#each recordings as rec (rec.id)}
      <li class="item">
        <div class="item__info">
          <span class="item__name">{rec.customTitle ?? rec.fileName}</span>
          <span class="item__meta">
            {formatSize(rec.fileSize)}{formatDuration(rec.durationSeconds)
              ? ` · ${formatDuration(rec.durationSeconds)}`
              : ''}
            {#if rec.uploaded}
              &middot; <span class="badge--uploaded">Uploaded</span>
            {/if}
            {#if rec.videoUrl}
              &middot; <a href={rec.videoUrl} target="_blank" rel="noopener noreferrer">Watch</a>
            {/if}
            {#if getUploadBadge(rec)}
              {@const badge = getUploadBadge(rec)!}
              &middot; <span class="upload-badge upload-badge--{badge.variant}">{badge.label}</span>
            {/if}
          </span>
        </div>
        <div class="item__actions">
          {#if confirmingDeleteId === rec.id}
            <span class="confirm-label">Also delete the file?</span>
            <button
              class="btn-danger"
              onclick={() => handleDelete(rec.id, true)}
              disabled={deletingId === rec.id}>Record + File</button
            >
            <button
              class="btn-secondary"
              onclick={() => handleDelete(rec.id, false)}
              disabled={deletingId === rec.id}>Record only</button
            >
            <button
              class="btn-cancel"
              onclick={() => (confirmingDeleteId = null)}
              disabled={deletingId === rec.id}>Cancel</button
            >
          {:else}
            {#if onassign}
              <button class="btn-assign" onclick={() => onassign?.(rec.id)}>
                Assign to Event
              </button>
            {/if}
            {#if ondelete}
              <button class="btn-delete" onclick={() => (confirmingDeleteId = rec.id)}>
                Delete
              </button>
            {/if}
          {/if}
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.75rem;
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
    font-weight: 500;
    font-size: 0.875rem;
    color: var(--text-primary);
  }

  .item__meta {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .item__actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .confirm-label {
    font-size: 0.8rem;
    color: var(--status-warn-text);
  }

  .badge--uploaded {
    color: var(--status-ok-text);
    font-weight: 500;
  }

  .upload-badge {
    font-weight: 500;
  }
  .upload-badge--ok {
    color: var(--status-ok-text);
  }
  .upload-badge--info {
    color: var(--accent);
  }
  .upload-badge--warn {
    color: var(--status-warn-text);
  }
  .upload-badge--err {
    color: var(--status-err-text);
  }
  .upload-badge--none {
    color: var(--text-secondary);
  }

  .btn-assign {
    padding: 0.375rem 0.875rem;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .btn-assign:hover {
    filter: brightness(0.9);
  }

  .btn-delete {
    padding: 0.375rem 0.875rem;
    background: transparent;
    color: var(--status-err-text);
    border: 1px solid var(--status-err-text);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-delete:hover {
    background: var(--status-err-bg);
  }

  .btn-danger {
    padding: 0.375rem 0.875rem;
    background: var(--status-err-dot);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-danger:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-danger:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-secondary {
    padding: 0.375rem 0.875rem;
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--nav-item-hover);
  }

  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-cancel {
    padding: 0.375rem 0.875rem;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-cancel:hover:not(:disabled) {
    color: var(--text-primary);
  }

  .btn-cancel:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .empty {
    color: var(--text-secondary);
  }
</style>
