<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { NotificationBell, Spinner, Tooltip } from '@metocast/design-system';
  import {
    activeIssueCount,
    unreadHistoryCount,
    topTier,
    openCentre,
  } from '$lib/notifications.svelte';
  import { live } from '$lib/live.svelte';

  // Every page puts the bell in its header, so the slide library's indexing shows beside it.
  const indexing = $derived(
    live.slideIndexPending > 0
      ? $_('library.indexing', { values: { n: live.slideIndexPending } })
      : null,
  );
</script>

<span class="trailing">
  {#if indexing}
    <Tooltip text={indexing} placement="bottom">
      {#snippet children({ describedby })}
        <span class="indexing" aria-describedby={describedby}>
          <Spinner size={18} label={indexing} />
        </span>
      {/snippet}
    </Tooltip>
  {/if}
  <NotificationBell
    label={activeIssueCount() === 0
      ? $_('notif.bell', { values: { n: unreadHistoryCount() } })
      : $_(activeIssueCount() === 1 ? 'notif.bellIssue' : 'notif.bellIssues', {
          values: { issues: activeIssueCount(), n: unreadHistoryCount() },
        })}
    unread={activeIssueCount() || unreadHistoryCount()}
    tier={topTier()}
    onclick={openCentre}
  />
</span>

<style>
  .trailing {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .indexing {
    display: inline-flex;
    color: var(--text-muted);
  }
</style>
