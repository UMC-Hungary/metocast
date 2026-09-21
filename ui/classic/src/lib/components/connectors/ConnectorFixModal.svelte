<script lang="ts">
  import ConnectorSettingsBlock from './ConnectorSettingsBlock.svelte';

  interface Props {
    connectorId: string;
    onClose: () => void;
  }

  let { connectorId, onClose }: Props = $props();

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div class="overlay" role="presentation">
  <div class="modal" role="dialog" aria-modal="true" aria-label="Fix connector settings">
    <div class="modal-header">
      <h2 class="modal-title">Fix Connector Settings</h2>
      <button class="close-btn" onclick={onClose} aria-label="Close modal">&times;</button>
    </div>
    <div class="modal-body">
      <ConnectorSettingsBlock {connectorId} onSaveSuccess={onClose} />
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }

  .modal {
    background: var(--modal-card-bg);
    border-radius: 0.5rem;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
    width: min(560px, calc(100vw - 2rem));
    max-height: calc(100vh - 4rem);
    overflow-y: auto;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.25rem 0;
  }

  .modal-title {
    margin: 0;
    font-size: 1.125rem;
  }

  .close-btn {
    background: none;
    border: none;
    font-size: 1.5rem;
    line-height: 1;
    cursor: pointer;
    color: var(--text-secondary);
    padding: 0.25rem;
  }

  .close-btn:hover {
    color: var(--text-primary);
  }

  .modal-body {
    padding: 1rem 0.25rem 0.25rem;
  }
</style>
