<script lang="ts">
  import IconButton from '../../src/primitives/IconButton.svelte';
  import Tooltip from '../../src/primitives/Tooltip.svelte';

  interface Props {
    text: string;
    placement?: 'top' | 'bottom';
  }

  let { text, placement = 'top' }: Props = $props();
</script>

<div class="frame">
  <Tooltip {text} {placement}>
    {#snippet children({ describedby })}
      <span aria-describedby={describedby}>
        <IconButton icon="info" label="Mit csinál ez?" />
      </span>
    {/snippet}
  </Tooltip>
</div>

<style>
  /* Space above and below, so a tooltip in either placement has somewhere to sit. */
  .frame {
    display: flex;
    justify-content: center;
    padding: var(--space-64) 0;
  }
</style>
