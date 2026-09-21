<script lang="ts">
  import Tabs, { type Tab } from '../../src/primitives/Tabs.svelte';

  interface Props {
    tabs: Tab[];
    value: string;
  }

  let { tabs, value = $bindable() }: Props = $props();
</script>

<Tabs {tabs} bind:value label="Esemény nézetek">
  {#snippet children(selected)}
    <p>A(z) <strong>{selected}</strong> panel tartalma.</p>
  {/snippet}
</Tabs>

<style>
  p {
    margin: 0;
    padding: var(--ui-gutter);
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    color: var(--text-primary);
  }
</style>
