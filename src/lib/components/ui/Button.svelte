<script>
  /**
   * @type {{
   *   variant?: 'primary' | 'secondary',
   *   size?: 'sm' | 'md' | 'lg',
   *   type?: 'button' | 'submit' | 'reset',
   *   disabled?: boolean,
   *   fullWidth?: boolean,
   *   flex?: boolean,
   *   class?: string,
   *   title?: string,
   *   ariaLabel?: string,
   *   onclick?: (e: MouseEvent) => void,
   *   children: import('svelte').Snippet,
   *   [key: string]: unknown
   * }}
   */
  let {
    variant = "primary",
    size = "md",
    type = "button",
    disabled = false,
    fullWidth = false,
    flex = false,
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
  class="btn btn-{variant} btn-{size} {className}"
  class:btn-full={fullWidth}
  class:btn-flex={flex}
  {onclick}
  {...rest}
>
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    font-family: var(--font);
    font-weight: var(--weight-semibold);
    cursor: pointer;
    white-space: nowrap;
    transition:
      background var(--transition),
      color var(--transition),
      border-color var(--transition),
      box-shadow var(--transition);
  }

  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .btn:disabled {
    opacity: var(--opacity-muted);
    cursor: not-allowed;
  }

  .btn-full { width: 100%; }
  .btn-flex { flex: 1; min-width: 0; }

  .btn-sm {
    height: var(--control-sm);
    padding: 0 var(--space-6);
    font-size: var(--font-sm);
    border-radius: var(--radius-sm);
  }

  .btn-md {
    height: var(--control-md);
    padding: 0 var(--space-8);
    font-size: var(--font-md);
    border-radius: var(--radius-md);
  }

  .btn-lg {
    height: var(--control-lg);
    padding: 0 var(--space-10);
    font-size: var(--font-base);
    border-radius: var(--radius-md);
  }

  .btn-primary {
    background: var(--accent);
    color: var(--on-accent);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .btn-primary:active:not(:disabled) {
    background: var(--accent-active);
    transform: scale(0.99);
  }

  .btn-primary:disabled {
    background: var(--surface);
    color: var(--text-secondary);
    border-color: var(--border);
    opacity: var(--opacity-soft);
  }

  .btn-secondary {
    background: var(--surface);
    color: var(--text-secondary);
    border-color: var(--border);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text);
    border-color: var(--border-hover);
  }

  .btn-secondary:active:not(:disabled) {
    transform: scale(0.99);
  }
</style>
