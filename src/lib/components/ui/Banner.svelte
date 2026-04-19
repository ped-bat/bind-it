<script>
  import IconButton from "./IconButton.svelte";

  /**
   * @type {{
   *   variant?: 'error' | 'warning' | 'info',
   *   message: string,
   *   retry?: boolean,
   *   dismissing?: boolean,
   *   dismissible?: boolean,
   *   dismissLabel?: string,
   *   dismissTitle?: string,
   *   ondismiss?: () => void,
   *   onretry?: () => void
   * }}
   */
  let {
    variant = "error",
    message,
    retry = false,
    dismissing = false,
    dismissible = true,
    dismissLabel = "Dismiss",
    dismissTitle,
    ondismiss,
    onretry,
  } = $props();
</script>

<div class="banner banner-{variant}" class:dismissing role={variant === "info" ? "status" : "alert"}>
  <div class="banner-content">
    {#each message.split("\n") as line}
      <span class="banner-line">{line}</span>
    {/each}
    {#if retry}
      {#if onretry}
        <button class="banner-retry-btn" onclick={onretry}>Retry</button>
      {:else}
        <span class="banner-retry">Fix the issue above, then try again.</span>
      {/if}
    {/if}
  </div>
  {#if dismissible && ondismiss}
    <IconButton
      variant="ghost"
      size="sm"
      shape="circle"
      class="banner-dismiss"
      onclick={ondismiss}
      title={dismissTitle}
      ariaLabel={dismissLabel}
    >
      {#snippet children()}&times;{/snippet}
    </IconButton>
  {/if}
</div>

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-6);
    border-radius: var(--radius-md);
    border-left-width: 3px;
    font-size: var(--font-sm);
    line-height: var(--leading-normal);
    flex-shrink: 0;
    animation: fadeIn var(--transition);
    overflow: hidden;
  }

  .banner-error {
    background: var(--error-ghost-12);
    border: 1px solid var(--error-ghost-30);
    border-left: 3px solid var(--error);
    color: var(--error);
  }

  .banner-warning {
    background: var(--accent-ghost-12);
    border: 1px solid var(--accent-ghost-30);
    border-left: 3px solid var(--accent);
    color: var(--accent);
  }

  .banner-info {
    background: var(--accent-ghost-4);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    color: var(--text-secondary);
  }

  .banner.dismissing {
    animation: bannerDismiss 400ms ease-out forwards;
  }

  @keyframes bannerDismiss {
    0% { opacity: 1; max-height: 200px; }
    50% { opacity: 0; max-height: 200px; }
    100% { opacity: 0; max-height: 0; padding-top: 0; padding-bottom: 0; border-width: 0; }
  }

  .banner-content {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    flex: 1;
    min-width: 0;
  }

  .banner-line { display: block; }

  .banner-retry {
    display: block;
    margin-top: var(--space-2);
    opacity: var(--opacity-soft);
    font-style: italic;
  }

  .banner-retry-btn {
    margin-top: var(--space-3);
    align-self: flex-start;
    background: var(--error-ghost-15);
    border: 1px solid var(--error-ghost-30);
    color: inherit;
    font-family: var(--font);
    font-size: var(--font-sm);
    font-weight: var(--weight-semibold);
    padding: var(--space-2) var(--space-5);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition);
  }

  .banner-retry-btn:hover { background: var(--error-ghost-30); }
  .banner-retry-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  .banner :global(.banner-dismiss) { color: inherit; flex-shrink: 0; margin-top: -2px; }
</style>
