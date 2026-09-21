<script lang="ts">
  import { assertiveMessage, politeMessage } from './live-region.js';
</script>

<div class="sr-only" aria-live="polite" aria-atomic="true">{$politeMessage}</div>
<div class="sr-only" aria-live="assertive" aria-atomic="true">{$assertiveMessage}</div>

<style>
  .sr-only {
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
