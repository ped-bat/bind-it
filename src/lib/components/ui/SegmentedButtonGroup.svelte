<script>
  /**
   * @template T
   * @typedef {{ value: T, label: string }} Option
   */

  /**
   * @type {{
   *   value: any,
   *   options: { value: any, label: string, disabled?: boolean }[],
   *   ariaLabel?: string,
   *   onchange?: (v: any) => void,
   * }}
   */
  let { value = $bindable(), options, ariaLabel, onchange } = $props();

  /** @param {any} v @param {boolean | undefined} disabled */
  function pick(v, disabled) {
    if (disabled) return;
    value = v;
    onchange?.(v);
  }
</script>

<div class="segmented" role="group" aria-label={ariaLabel}>
  {#each options as opt}
    <button
      type="button"
      class="segmented-btn"
      class:active={value === opt.value}
      aria-pressed={value === opt.value}
      disabled={opt.disabled}
      onclick={() => pick(opt.value, opt.disabled)}
    >{opt.label}</button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: var(--space-1);
    gap: var(--space-1);
    height: var(--control-md);
    box-sizing: border-box;
  }

  .segmented-btn {
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-family: var(--font);
    font-size: var(--font-md);
    font-weight: var(--weight-medium);
    padding: 0 var(--space-6);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition), color var(--transition);
  }

  .segmented-btn:hover { color: var(--text); }
  .segmented-btn.active { background: var(--accent); color: var(--on-accent); }
  .segmented-btn.active:hover { color: var(--on-accent); }
  .segmented-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .segmented-btn:disabled {
    cursor: not-allowed;
    opacity: var(--opacity-faint);
  }
  .segmented-btn:disabled:hover { color: var(--text-secondary); }
</style>
