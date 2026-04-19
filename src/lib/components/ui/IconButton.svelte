<script>
  /**
   * @type {{
   *   variant?: 'ghost' | 'danger',
   *   size?: 'sm' | 'md',
   *   shape?: 'circle' | 'square',
   *   type?: 'button' | 'submit',
   *   disabled?: boolean,
   *   class?: string,
   *   title?: string,
   *   ariaLabel: string,
   *   onclick?: (e: MouseEvent) => void,
   *   children: import('svelte').Snippet,
   *   [key: string]: unknown
   * }}
   */
  let {
    variant = "ghost",
    size = "md",
    shape = "square",
    type = "button",
    disabled = false,
    class: className = "",
    title,
    ariaLabel,
    onclick,
    children,
    ...rest
  } = $props();
</script>

<button
  {type}
  {disabled}
  {title}
  aria-label={ariaLabel}
  class="icon-btn icon-btn-{variant} icon-btn-{size} icon-btn-{shape} {className}"
  {onclick}
  {...rest}
>
  {@render children()}
</button>

<style>
  .icon-btn {
    background: none;
    border: 1px solid transparent;
    color: var(--text-secondary);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    line-height: 1;
    transition:
      background var(--transition),
      color var(--transition),
      border-color var(--transition);
  }

  .icon-btn:disabled {
    opacity: var(--opacity-muted);
    cursor: not-allowed;
  }

  .icon-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .icon-btn-sm {
    width: 24px;
    height: 24px;
    font-size: var(--font-sm);
  }

  .icon-btn-md {
    width: var(--control-sm);
    height: var(--control-sm);
    font-size: var(--font-lg);
  }

  .icon-btn-square { border-radius: var(--radius-md); }
  .icon-btn-circle { border-radius: var(--radius-full); }

  .icon-btn-ghost:hover:not(:disabled) {
    background: var(--border-ghost-60);
    color: var(--text);
  }

  .icon-btn-danger:hover:not(:disabled) {
    background: var(--error-ghost-12);
    color: var(--error);
  }
</style>
