<script lang="ts">
  import {
    obsConfig,
    obsStatus,
    vmixConfig,
    vmixStatus,
    atemConfig,
    atemStatus,
    youtubeConfig,
    youtubeStatus,
    facebookConfig,
    facebookStatus,
    youtubeLiveActive,
  } from '$lib/stores/connectors.js';
  import ConnectorStatusBadge from '$lib/components/connectors/ConnectorStatusBadge.svelte';
</script>

{#if $obsConfig.enabled || $vmixConfig.enabled || $atemConfig.enabled || $youtubeConfig.enabled || $facebookConfig.enabled || $youtubeLiveActive}
  <div class="nav-connectors">
    {#if $obsConfig.enabled}
      <ConnectorStatusBadge name="OBS" status={$obsStatus} />
    {/if}
    {#if $vmixConfig.enabled}
      <ConnectorStatusBadge name="VMix" status={$vmixStatus} />
    {/if}
    {#if $atemConfig.enabled}
      <ConnectorStatusBadge name="ATEM" status={$atemStatus} />
    {/if}
    {#if $youtubeConfig.enabled}
      <ConnectorStatusBadge name="YouTube" status={$youtubeStatus} />
    {/if}
    {#if $facebookConfig.enabled}
      <ConnectorStatusBadge name="Facebook" status={$facebookStatus} />
    {/if}
    {#if $youtubeLiveActive}
      <span class="yt-live-badge" aria-label="YouTube is live">
        <span class="yt-live-dot" aria-hidden="true"></span>
        YouTube LIVE
      </span>
    {/if}
  </div>
{/if}

<style>
  .nav-connectors {
    display: flex;
    gap: 0.5rem;
    margin-left: auto;
    align-items: center;
  }

  .yt-live-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.2rem 0.6rem;
    border-radius: 9999px;
    font-size: 0.8rem;
    font-weight: 600;
    background: var(--status-err-bg);
    color: var(--status-err-text);
    letter-spacing: 0.02em;
  }

  .yt-live-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--status-err-dot);
    flex-shrink: 0;
    animation: live-pulse 1.2s ease-in-out infinite;
  }

  @keyframes live-pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(0.75);
    }
  }
</style>
