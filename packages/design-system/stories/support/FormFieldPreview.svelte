<script lang="ts">
  import FormField from '../../src/primitives/FormField.svelte';
  import Select from '../../src/primitives/Select.svelte';
  import TextArea from '../../src/primitives/TextArea.svelte';
  import TextField from '../../src/primitives/TextField.svelte';

  interface Props {
    label: string;
    hint?: string;
    error?: string;
    required?: boolean;
    control?: 'text' | 'textarea' | 'select';
    value?: string;
  }

  let {
    label,
    hint = '',
    error = '',
    required = false,
    control = 'text',
    value = $bindable(''),
  }: Props = $props();

  const options = [
    { value: 'worship', label: 'Dicsőítés' },
    { value: 'sermon', label: 'Igehirdetés' },
    { value: 'prayer', label: 'Imádság' },
  ];
</script>

<FormField {label} {hint} {error} {required}>
  {#snippet children({ controlId, describedby, invalid })}
    {#if control === 'textarea'}
      <TextArea id={controlId} {describedby} {invalid} bind:value />
    {:else if control === 'select'}
      <Select id={controlId} {describedby} {invalid} {options} value="worship" />
    {:else}
      <TextField id={controlId} {describedby} {invalid} bind:value />
    {/if}
  {/snippet}
</FormField>
