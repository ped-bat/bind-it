<script>
  /** @type {{ size?: 'sm' | 'md', children: import('svelte').Snippet }} */
  let { size = "md", children } = $props();
</script>

<span class="field-label" class:sm={size === "sm"}>{@render children()}</span>

<style>
  .field-label {
    font-size: var(--font-md);
    color: var(--text-secondary);
    font-weight: var(--weight-medium);
    line-height: var(--leading-normal);
  }

  .field-label.sm {
    font-size: var(--font-xs);
  }
</style>
