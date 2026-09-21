<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { authToken } from '$lib/stores/server-url.js';
  import { listCronJobs, createCronJob, updateCronJob, deleteCronJob } from '@metocast/core-client';
  import type { CronJob } from '@metocast/core-client';

  const emptyForm = () => ({
    name: '',
    cronExpression: '',
    enabled: true,
    pullYoutube: false,
    autoUpload: false,
  });

  let cronJobs: CronJob[] = $state([]);
  let cronForm = $state(emptyForm());
  let editingCronId: string | null = $state(null);
  let saving = $state(false);
  let error = $state('');

  $effect(() => {
    if ($authToken) loadCronJobs();
  });

  async function loadCronJobs() {
    try {
      cronJobs = await listCronJobs();
    } catch {
      // server might not be running in client mode — silently ignore
    }
  }

  function startEdit(job: CronJob) {
    editingCronId = job.id;
    cronForm = {
      name: job.name,
      cronExpression: job.cronExpression,
      enabled: job.enabled,
      pullYoutube: job.pullYoutube,
      autoUpload: job.autoUpload,
    };
  }

  function cancelEdit() {
    editingCronId = null;
    cronForm = emptyForm();
    error = '';
  }

  async function save() {
    saving = true;
    error = '';
    try {
      if (editingCronId) {
        await updateCronJob(editingCronId, cronForm);
      } else {
        await createCronJob(cronForm);
      }
      cancelEdit();
      await loadCronJobs();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function remove(id: string) {
    error = '';
    try {
      await deleteCronJob(id);
      await loadCronJobs();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section>
  <h2>{$_('appSettings.cronJobs.title')}</h2>

  {#if cronJobs.length > 0}
    <table class="cron-table">
      <thead>
        <tr>
          <th scope="col">{$_('appSettings.cronJobs.colName')}</th>
          <th scope="col">{$_('appSettings.cronJobs.colExpression')}</th>
          <th scope="col">{$_('appSettings.cronJobs.colEnabled')}</th>
          <th scope="col">{$_('appSettings.cronJobs.colPullYoutube')}</th>
          <th scope="col">Auto Upload</th>
          <th scope="col"><span class="sr-only">{$_('appSettings.cronJobs.colActions')}</span></th>
        </tr>
      </thead>
      <tbody>
        {#each cronJobs as job (job.id)}
          <tr>
            <td>{job.name}</td>
            <td><code>{job.cronExpression}</code></td>
            <td>{job.enabled ? '✓' : '✗'}</td>
            <td>{job.pullYoutube ? '✓' : '✗'}</td>
            <td>{job.autoUpload ? '✓' : '✗'}</td>
            <td class="cron-actions">
              <button class="btn-secondary btn-sm" onclick={() => startEdit(job)}>
                {$_('appSettings.cronJobs.editButton')}
              </button>
              <button class="btn-danger btn-sm" onclick={() => remove(job.id)}>
                {$_('appSettings.cronJobs.deleteButton')}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="note">{$_('appSettings.cronJobs.noneConfigured')}</p>
  {/if}

  <h3 class="form-title">
    {editingCronId ? $_('appSettings.cronJobs.editTitle') : $_('appSettings.cronJobs.addTitle')}
  </h3>

  <div class="form-grid">
    <div class="field field--full">
      <label for="cron-name">{$_('appSettings.cronJobs.colName')}</label>
      <input
        id="cron-name"
        type="text"
        bind:value={cronForm.name}
        placeholder={$_('appSettings.cronJobs.namePlaceholder')}
      />
    </div>
    <div class="field field--full">
      <label for="cron-expr">{$_('appSettings.cronJobs.expressionLabel')}</label>
      <input
        id="cron-expr"
        type="text"
        bind:value={cronForm.cronExpression}
        placeholder={$_('appSettings.cronJobs.expressionPlaceholder')}
      />
    </div>
  </div>

  <div class="form-row">
    <label class="checkbox-label">
      <input type="checkbox" bind:checked={cronForm.enabled} />
      {$_('appSettings.cronJobs.enabledLabel')}
    </label>
  </div>
  <div class="form-row">
    <label class="checkbox-label">
      <input type="checkbox" bind:checked={cronForm.pullYoutube} />
      {$_('appSettings.cronJobs.pullYoutubeLabel')}
    </label>
  </div>
  <div class="form-row">
    <label class="checkbox-label">
      <input type="checkbox" bind:checked={cronForm.autoUpload} />
      Auto-upload flagged recordings
    </label>
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <div class="button-row">
    <button
      class="btn-primary"
      onclick={save}
      disabled={saving || !cronForm.name || !cronForm.cronExpression}
    >
      {saving ? $_('appSettings.cronJobs.saving') : $_('appSettings.cronJobs.save')}
    </button>
    {#if editingCronId}
      <button class="btn-secondary" onclick={cancelEdit}>
        {$_('appSettings.cronJobs.cancel')}
      </button>
    {/if}
  </div>
</section>

<style>
  section {
    padding: 1.25rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  h2 {
    font-size: 1.125rem;
    margin: 0 0 0.75rem;
  }

  .note {
    font-size: 0.875rem;
    color: var(--text-secondary);
    margin: 0.5rem 0 1rem;
  }

  .error {
    color: var(--status-err-text);
    font-size: 0.875rem;
    margin: 0 0 0.75rem;
  }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .field--full {
    grid-column: 1 / -1;
  }

  .field label,
  .checkbox-label {
    font-size: 0.875rem;
    color: var(--text-primary);
    font-weight: 500;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .form-row {
    margin-bottom: 0.75rem;
  }

  input[type='text'] {
    padding: 0.375rem 0.625rem;
    border: 1px solid var(--input-border);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    width: 100%;
    box-sizing: border-box;
  }

  input[type='text']:focus {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-color: var(--accent);
  }

  .button-row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .btn-primary {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-secondary {
    padding: 0.5rem 1rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--accent-subtle);
  }

  .btn-danger {
    padding: 0.5rem 1rem;
    background: var(--status-err-dot);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .btn-danger:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-sm {
    padding: 0.25rem 0.625rem;
    font-size: 0.8125rem;
  }

  .cron-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
    margin-bottom: 1.25rem;
  }

  .cron-table th,
  .cron-table td {
    text-align: left;
    padding: 0.375rem 0.5rem;
    border-bottom: 1px solid var(--border);
  }

  .cron-table th {
    font-weight: 600;
    color: var(--text-primary);
  }

  .cron-table code {
    font-family: ui-monospace, monospace;
    font-size: 0.8125rem;
    background: var(--content-bg);
    padding: 0.125rem 0.25rem;
    border-radius: 0.25rem;
  }

  .cron-actions {
    display: flex;
    gap: 0.375rem;
  }

  .form-title {
    font-size: 0.9375rem;
    margin: 0 0 0.75rem;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
