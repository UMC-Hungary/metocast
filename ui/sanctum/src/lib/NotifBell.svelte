<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { NotificationBell } from '@metocast/design-system';
  import {
    activeIssueCount,
    unreadHistoryCount,
    topTier,
    openCentre,
  } from '$lib/notifications.svelte';
</script>

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
