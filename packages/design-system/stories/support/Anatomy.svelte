<script lang="ts">
  import ReferencePanel from './ReferencePanel.svelte';

  interface Props {
    component: string;
  }

  let { component }: Props = $props();
</script>

<ReferencePanel {component} />
