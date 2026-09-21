<script lang="ts">
  import { referenceFor } from './reference.js';

  interface Props {
    component: string;
  }

  let { component }: Props = $props();
  const record = $derived(referenceFor(component));
</script>

{#if record}
  <section class="sanctum-reference">
    <h3>Reference · {record.source}</h3>
    <table>
      <thead>
        <tr>
          <th>Selector</th>
          <th>Property</th>
          <th>Measured</th>
          <th>Implemented</th>
          <th>Token</th>
        </tr>
      </thead>
      <tbody>
        {#each record.rows as row (row.token + row.selector + row.property)}
          <tr class:mismatch={row.measured !== row.implemented}>
            <td>{row.selector}</td>
            <td>{row.property}</td>
            <td>{row.measured}</td>
            <td>{row.implemented}</td>
            <td>--{row.token}</td>
          </tr>
          {#if row.deviation}
            <tr class="deviation"><td colspan="5">Deviation — {row.deviation}</td></tr>
          {/if}
        {/each}
      </tbody>
    </table>
  </section>
{:else}
  <section class="sanctum-reference">
    <h3>No reference counterpart</h3>
    <p>
      This component has no counterpart in the design reference. Review it against its neighbours
      rather than against a source.
    </p>
  </section>
{/if}

<style>
  .mismatch td {
    color: var(--status-warn);
  }
</style>
