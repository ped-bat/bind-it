<script>
  import FieldLabel from "./FieldLabel.svelte";

  /**
   * @type {{
   *   label: string,
   *   labelSize?: 'sm' | 'md',
   *   class?: string,
   *   children: import('svelte').Snippet
   * }}
   */
  let { label, labelSize = "md", class: className = "", children } = $props();
</script>

<label class="field {className}">
  <FieldLabel size={labelSize}>{#snippet children()}{label}{/snippet}</FieldLabel>
  {@render children()}
</label>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
</style>
