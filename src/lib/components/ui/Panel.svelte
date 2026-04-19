<script>
  /** @type {{ title: string, children: import('svelte').Snippet, actions?: import('svelte').Snippet }} */
  let { title, children, actions } = $props();
</script>

<section class="panel">
  <div class="panel-header" class:has-actions={!!actions}>
    <h2>{title}</h2>
    {#if actions}{@render actions()}{/if}
  </div>
  {@render children()}
</section>

<style>
  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--space-8);
  }

  .panel-header {
    display: flex;
    align-items: center;
    margin-bottom: var(--space-8);
    min-height: var(--control-sm);
  }

  .panel-header.has-actions {
    justify-content: space-between;
    gap: var(--space-4);
  }

  h2 {
    font-family: var(--font-display);
    font-size: var(--font-2xl);
    font-weight: var(--weight-regular);
    letter-spacing: -0.01em;
    color: var(--text);
    margin: 0;
    line-height: var(--leading-tight);
  }
</style>
