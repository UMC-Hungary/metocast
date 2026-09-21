<script lang="ts" module>
  export interface Column<Row> {
    key: string;
    header: string;
    /** Right-align a numeric column; the reference sets figures in tabular numerals. */
    numeric?: boolean;
    /** Hidden below the reflow threshold, where every cell becomes a labelled line. */
    cell: (row: Row) => string;
  }
</script>

<script lang="ts" generics="Row extends { id: string }">
  interface Props {
    columns: Column<Row>[];
    rows: Row[];
    /** Names the table. A table without one is a grid of unexplained values. */
    caption: string;
    /** Shows the caption rather than leaving it to assistive technology only. */
    captionVisible?: boolean;
  }

  let { columns, rows, caption, captionVisible = false }: Props = $props();
</script>

<div class="scroll">
  <table>
    <caption class:visually-hidden={!captionVisible}>{caption}</caption>
    <thead class="table-head">
      <tr>
        {#each columns as column (column.key)}
          <th scope="col" class:numeric={column.numeric}>{column.header}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.id)}
        <tr>
          {#each columns as column (column.key)}
            <td class:numeric={column.numeric} data-label={column.header}>{column.cell(row)}</td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .scroll {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    background: var(--surface-raised);
  }

  caption {
    text-align: left;
    padding: var(--c-table-cell-padding-block) var(--ui-gutter);
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--type-label-track);
    text-transform: var(--type-label-transform);
    color: var(--text-muted);
  }

  th {
    text-align: left;
    padding: var(--c-table-cell-padding-block) var(--ui-gutter);
    border-bottom: var(--ui-border-hairline) solid var(--border-strong);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--type-label-sm-track);
    text-transform: var(--type-label-sm-transform);
    font-weight: var(--type-label-sm-weight);
    color: var(--text-muted);
    white-space: nowrap;
  }

  td {
    padding: var(--c-table-cell-padding-block) var(--ui-gutter);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
    font-family: var(--type-body-sm-family);
    font-size: var(--type-body-sm-size);
    letter-spacing: var(--type-body-sm-track);
    color: var(--text-primary);
  }

  .numeric {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  /*
   * Below the reflow threshold each row becomes a stack of labelled lines, so no information
   * and no action is lost — the alternative, a horizontally scrolling grid, hides both.
   */
  @media (max-width: 420px) {
    .table-head {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }

    tr {
      display: block;
      border-bottom: var(--ui-border-hairline) solid var(--border-strong);
    }

    td {
      display: flex;
      justify-content: space-between;
      gap: var(--ui-stack);
      border-bottom: 0;
      padding: var(--c-table-stacked-padding-block) var(--ui-gutter);
    }

    td::before {
      content: attr(data-label);
      font-family: var(--type-label-sm-family);
      font-size: var(--type-label-sm-size);
      letter-spacing: var(--type-label-sm-track);
      text-transform: var(--type-label-sm-transform);
      color: var(--text-muted);
    }

    .numeric {
      text-align: right;
    }
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
</style>
