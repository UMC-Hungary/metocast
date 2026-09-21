<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { getEvent, deleteEvent } from '@metocast/core-client';
  import { listRecordings, deleteRecording } from '@metocast/core-client';
  import { triggerYouTubeSchedule, triggerFacebookSchedule } from '@metocast/core-client';
  import { listActivities, createActivity, deleteActivity } from '@metocast/core-client';
  import type { Event, EventActivity } from '@metocast/core-client/schemas/event';
  import type { Recording } from '@metocast/core-client/schemas/recording';
  import RecordingList from '$lib/components/recordings/RecordingList.svelte';
  import CreateRecordingForm from '$lib/components/recordings/CreateRecordingForm.svelte';
  import UploadModal from '$lib/components/recordings/UploadModal.svelte';
  import { youtubeStatus, facebookStatus } from '$lib/stores/connectors.js';

  let event = $state<Event | null>(null);
  let recordings = $state<Recording[]>([]);
  let activities = $state<EventActivity[]>([]);
  let loadingEvent = $state(true);
  let loadingRecordings = $state(true);
  let error = $state('');
  let showAddRecording = $state(false);
  let schedulingYt = $state(false);
  let schedulingFb = $state(false);
  let scheduleError = $state('');
  let togglingCompletion = $state(false);
  let deletingEvent = $state(false);
  let showUploadModal = $state(false);

  const id = page.params.id ?? '';

  const isPast = $derived(event ? new Date(event.dateTime) <= new Date() : false);
  const isCompleted = $derived(activities.some((a) => a.activityType === 'completed'));
  const completedActivity = $derived(activities.find((a) => a.activityType === 'completed'));

  function conn(platform: string) {
    return event?.connections.find((c) => c.platform === platform);
  }

  onMount(async () => {
    try {
      [event, recordings, activities] = await Promise.all([
        getEvent(id),
        listRecordings(id),
        listActivities(id),
      ]);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingEvent = false;
      loadingRecordings = false;
    }
  });

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleString(undefined, {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  function handleRecordingCreated(rec: Recording) {
    recordings = [rec, ...recordings];
    showAddRecording = false;
  }

  async function scheduleYouTube() {
    if (!event) return;
    schedulingYt = true;
    scheduleError = '';
    try {
      await triggerYouTubeSchedule(event.id);
      // Reload event to get updated status
      event = await getEvent(id);
    } catch (e) {
      scheduleError = e instanceof Error ? e.message : String(e);
    } finally {
      schedulingYt = false;
    }
  }

  async function scheduleFacebook() {
    if (!event) return;
    schedulingFb = true;
    scheduleError = '';
    try {
      await triggerFacebookSchedule(event.id);
      event = await getEvent(id);
    } catch (e) {
      scheduleError = e instanceof Error ? e.message : String(e);
    } finally {
      schedulingFb = false;
    }
  }

  async function toggleCompletion() {
    togglingCompletion = true;
    try {
      if (isCompleted && completedActivity) {
        await deleteActivity(id, completedActivity.id);
        activities = activities.filter((a) => a.id !== completedActivity!.id);
      } else {
        const act = await createActivity(id, { activity_type: 'completed' });
        activities = [...activities, act];
        // Open upload modal if there are recordings and upload platforms are connected
        if (
          recordings.length > 0 &&
          ($youtubeStatus === 'connected' || $facebookStatus === 'connected')
        ) {
          showUploadModal = true;
        }
      }
    } catch (e) {
      scheduleError = e instanceof Error ? e.message : String(e);
    } finally {
      togglingCompletion = false;
    }
  }

  function handleUploadFlagged(updated: Recording[]) {
    recordings = updated;
  }

  async function handleDeleteEvent() {
    if (!confirm('Delete this event and all its recordings? This cannot be undone.')) return;
    deletingEvent = true;
    try {
      await deleteEvent(id);
      await goto('/events');
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      deletingEvent = false;
    }
  }

  async function handleDeleteRecording(recordingId: string, deleteFile: boolean) {
    await deleteRecording(id, recordingId, deleteFile);
    recordings = recordings.filter((r) => r.id !== recordingId);
  }
</script>

<svelte:head>
  <title>{event?.title ?? 'Event'} — Metocast</title>
</svelte:head>

<div class="nav">
  <a href="/events" class="back">&larr; Events</a>
  {#if event}
    <div class="nav__actions">
      <a href="/events/{id}/edit" class="btn-edit">Edit</a>
      <button class="btn-danger" onclick={handleDeleteEvent} disabled={deletingEvent}>
        {deletingEvent ? 'Deleting…' : 'Delete Event'}
      </button>
    </div>
  {/if}
</div>

{#if error}
  <p class="error" role="alert">{error}</p>
{:else if loadingEvent}
  <p>Loading…</p>
{:else if event}
  <h1>{event.title}</h1>

  <dl class="meta">
    {#if event.speaker}
      <dt>Speaker</dt>
      <dd>{event.speaker}</dd>
    {/if}
    <dt>Date &amp; Time</dt>
    <dd>{formatDate(event.dateTime)}</dd>
    {#if event.description}
      <dt>Description</dt>
      <dd>{event.description}</dd>
    {/if}
    <dt>YouTube Visibility</dt>
    <dd>
      {conn('youtube')?.privacyStatus === 'public'
        ? 'Public'
        : conn('youtube')?.privacyStatus === 'unlisted'
          ? 'Unlisted'
          : 'Private'}
    </dd>
    <dt>Facebook Visibility</dt>
    <dd>
      {conn('facebook')?.privacyStatus === 'EVERYONE'
        ? 'Public'
        : conn('facebook')?.privacyStatus === 'FRIENDS'
          ? 'Friends'
          : 'Only Me'}
    </dd>
  </dl>

  <!-- Social connector status & actions -->
  <section class="social">
    <h2>Social Platforms</h2>

    {#if scheduleError}
      <p class="error" role="alert">{scheduleError}</p>
    {/if}

    <div class="social-grid">
      <!-- YouTube -->
      <div class="social-item">
        <div class="social-item__header">
          <span class="social-item__name">YouTube</span>
          <span
            class="badge badge--{conn('youtube')?.scheduleStatus === 'scheduled' ||
            conn('youtube')?.externalId
              ? 'scheduled'
              : conn('youtube')?.scheduleStatus === 'failed'
                ? 'failed'
                : 'none'}"
          >
            {conn('youtube')?.scheduleStatus === 'scheduled' || conn('youtube')?.externalId
              ? 'Scheduled'
              : conn('youtube')?.scheduleStatus === 'failed'
                ? 'Failed'
                : 'Not Scheduled'}
          </span>
        </div>
        {#if conn('youtube')?.streamUrl}
          <a
            href={conn('youtube')?.streamUrl}
            target="_blank"
            rel="noopener noreferrer"
            class="view-link"
          >
            View on YouTube
          </a>
        {/if}
        {#if $youtubeStatus === 'connected' && conn('youtube')?.scheduleStatus !== 'scheduled' && !conn('youtube')?.externalId}
          <button
            class="btn-secondary"
            onclick={scheduleYouTube}
            disabled={schedulingYt || isPast}
            title={isPast ? 'Event date is in the past — edit the event to reschedule' : undefined}
          >
            {schedulingYt ? 'Scheduling…' : 'Schedule on YouTube'}
          </button>
          {#if isPast}
            <p class="past-warning">Event date has passed. Update the date to schedule.</p>
          {/if}
        {/if}
      </div>

      <!-- Facebook -->
      <div class="social-item">
        <div class="social-item__header">
          <span class="social-item__name">Facebook</span>
          <span
            class="badge badge--{conn('facebook')?.scheduleStatus === 'scheduled'
              ? 'scheduled'
              : conn('facebook')?.scheduleStatus === 'failed'
                ? 'failed'
                : 'none'}"
          >
            {conn('facebook')?.scheduleStatus === 'scheduled'
              ? 'Scheduled'
              : conn('facebook')?.scheduleStatus === 'failed'
                ? 'Failed'
                : 'Not Scheduled'}
          </span>
        </div>
        {#if conn('facebook')?.eventUrl}
          <a
            href={conn('facebook')?.eventUrl}
            target="_blank"
            rel="noopener noreferrer"
            class="view-link"
          >
            View on Facebook
          </a>
        {/if}
        {#if $facebookStatus === 'connected' && conn('facebook')?.scheduleStatus !== 'scheduled'}
          <button
            class="btn-secondary"
            onclick={scheduleFacebook}
            disabled={schedulingFb || isPast}
            title={isPast ? 'Event date is in the past — edit the event to reschedule' : undefined}
          >
            {schedulingFb ? 'Scheduling…' : 'Schedule on Facebook'}
          </button>
          {#if isPast}
            <p class="past-warning">Event date has passed. Update the date to schedule.</p>
          {/if}
        {/if}
      </div>
    </div>
  </section>

  <section class="status-section">
    <h2>Event Status</h2>
    <div class="status-row">
      <span class="status-label">
        {isCompleted ? 'This event is marked as completed.' : 'This event is not yet completed.'}
      </span>
      <button
        class="btn-status"
        class:btn-status--undo={isCompleted}
        onclick={toggleCompletion}
        disabled={togglingCompletion}
      >
        {#if togglingCompletion}
          …
        {:else if isCompleted}
          Undo Completion
        {:else}
          Mark as Completed
        {/if}
      </button>
    </div>
  </section>

  <section class="recordings">
    <div class="recordings__header">
      <h2>Recordings</h2>
      <button
        onclick={() => {
          showAddRecording = !showAddRecording;
        }}
      >
        {showAddRecording ? 'Cancel' : '+ Add Recording'}
      </button>
    </div>

    {#if showAddRecording}
      <div class="recordings__form">
        <CreateRecordingForm eventId={id} oncreated={handleRecordingCreated} />
      </div>
    {/if}

    <RecordingList {recordings} loading={loadingRecordings} ondelete={handleDeleteRecording} />
  </section>
{/if}

{#if showUploadModal && event}
  <UploadModal
    {event}
    {recordings}
    onclose={() => (showUploadModal = false)}
    onuploaded={handleUploadFlagged}
  />
{/if}

<style>
  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  .nav__actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .back {
    color: var(--text-secondary);
    text-decoration: none;
    font-size: 0.875rem;
  }
  .back:hover {
    color: var(--text-primary);
  }

  .btn-edit {
    padding: 0.375rem 0.875rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    text-decoration: none;
    font-size: 0.875rem;
    font-weight: 500;
  }
  .btn-edit:hover {
    background: var(--accent-subtle);
  }

  .btn-danger {
    padding: 0.375rem 0.875rem;
    background: transparent;
    color: var(--status-err-text);
    border: 1px solid var(--status-err-text);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
  }
  .btn-danger:hover:not(:disabled) {
    background: var(--status-err-bg);
  }
  .btn-danger:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  h1 {
    margin-bottom: 1.5rem;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.5rem 1rem;
    margin-bottom: 2rem;
  }
  .meta dt {
    font-weight: 600;
    color: var(--text-primary);
  }
  .meta dd {
    margin: 0;
    color: var(--text-secondary);
  }

  .recordings__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }
  .recordings__header h2 {
    margin: 0;
  }
  .recordings__header button {
    padding: 0.5rem 1rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    font-size: 0.875rem;
  }
  .recordings__header button:hover {
    filter: brightness(0.9);
  }

  .recordings__form {
    padding: 1rem;
    background: var(--content-bg);
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  .error {
    padding: 0.75rem;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    border-radius: 0.375rem;
  }

  .social {
    margin-bottom: 2rem;
  }

  .social h2 {
    margin-bottom: 1rem;
  }

  .social-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .social-item {
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .social-item__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .social-item__name {
    font-weight: 600;
    color: var(--text-primary);
  }

  .badge {
    font-size: 0.75rem;
    padding: 0.2rem 0.5rem;
    border-radius: 9999px;
    font-weight: 500;
  }

  .badge--scheduled {
    background: var(--status-ok-bg);
    color: var(--status-ok-text);
  }

  .badge--failed {
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }

  .badge--none {
    background: var(--content-bg);
    color: var(--text-secondary);
  }

  .view-link {
    font-size: 0.875rem;
    color: var(--accent);
    text-decoration: none;
  }

  .view-link:hover {
    text-decoration: underline;
  }

  .past-warning {
    font-size: 0.75rem;
    color: var(--status-warn-text);
    background: var(--status-warn-bg);
    border: 1px solid var(--status-warn-dot);
    border-radius: 0.25rem;
    padding: 0.25rem 0.5rem;
    margin: 0;
  }

  .btn-secondary {
    padding: 0.375rem 0.875rem;
    background: transparent;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    align-self: flex-start;
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--accent-subtle);
  }

  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .status-section {
    margin-bottom: 2rem;
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
  }

  .status-section h2 {
    margin: 0 0 0.75rem;
  }

  .status-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .status-label {
    font-size: 0.875rem;
    color: var(--text-secondary);
  }

  .btn-status {
    padding: 0.375rem 0.875rem;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-status:hover:not(:disabled) {
    filter: brightness(0.9);
  }

  .btn-status--undo {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border);
  }

  .btn-status--undo:hover:not(:disabled) {
    background: var(--nav-item-hover);
  }

  .btn-status:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
