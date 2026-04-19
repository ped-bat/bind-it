<script>
  import { fade } from "svelte/transition";
  import { tick } from "svelte";
  import AppHeader from "$lib/components/ui/AppHeader.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { conversionStore } from "$lib/stores/conversion.svelte.js";
  import { formatDurationHuman, formatElapsed, formatBytes } from "$lib/services/format.js";
  import Icon from "$lib/components/ui/Icon.svelte";

  let revealPending = $state(false);
  /** @type {HTMLDivElement | undefined} */
  let actions;

  async function handleReveal() {
    if (revealPending) return;
    revealPending = true;
    try {
      await conversionStore.revealOutput();
    } finally {
      revealPending = false;
    }
  }

  $effect(() => {
    tick().then(() => {
      const btn = actions?.querySelector("button");
      if (btn instanceof HTMLButtonElement) btn.focus();
    });
  });
</script>

<div class="complete-screen" in:fade={{ duration: 300 }}>
  <header class="complete-header">
    <AppHeader size="sm" />
  </header>

  <div class="complete-content">
    <div class="complete-hero">
      <div class="complete-icon">
        <Icon name="check-circle" width={56} height={56} />
      </div>
      <h2 class="complete-title">Audiobook bound!</h2>
    </div>

    {#if conversionStore.completionData}
      <div class="complete-details">
        <p class="complete-filename">{conversionStore.completionData.filename}</p>
        <div class="complete-stats">
          <span>{conversionStore.completionData.fileCount} file{conversionStore.completionData.fileCount !== 1 ? "s" : ""}</span>
          <span class="complete-stat-sep">&middot;</span>
          <span>{formatDurationHuman(conversionStore.completionData.totalDuration)}</span>
          {#if conversionStore.completionData.sizeBytes}
            <span class="complete-stat-sep">&middot;</span>
            <span>{formatBytes(conversionStore.completionData.sizeBytes)}</span>
          {/if}
        </div>
        <p class="complete-processing-time">Processing time: {formatElapsed(conversionStore.completionData.elapsed)}</p>
      </div>
    {/if}

    <div class="complete-actions" bind:this={actions}>
      <Button variant="secondary" size="lg" flex onclick={() => conversionStore.convertAnother()}>
        {#snippet children()}Bind another{/snippet}
      </Button>
      <Button variant="primary" size="lg" flex disabled={revealPending} onclick={handleReveal}>
        {#snippet children()}Open folder{/snippet}
      </Button>
    </div>
  </div>
</div>

<style>
  .complete-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .complete-header {
    display: flex;
    justify-content: center;
    flex-shrink: 0;
  }

  .complete-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .complete-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-6);
    margin-bottom: var(--space-16);
  }

  .complete-details {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-16);
    max-width: 80%;
  }

  .complete-processing-time {
    font-size: var(--font-md);
    color: var(--text-secondary);
    margin: var(--space-2) 0 0 0;
  }

  .complete-icon {
    color: var(--accent);
    animation: springScale 0.6s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
  }

  @keyframes springScale {
    0% { transform: scale(0.3); opacity: 0; }
    60% { transform: scale(1.15); opacity: 1; }
    80% { transform: scale(0.95); }
    100% { transform: scale(1); opacity: 1; }
  }

  .complete-icon :global(.check-circle) {
    stroke-dasharray: 138;
    stroke-dashoffset: 138;
    animation: drawCircle 0.6s ease-out 0.1s forwards;
  }

  .complete-icon :global(.check-path) {
    stroke-dasharray: 30;
    stroke-dashoffset: 30;
    animation: drawCheck 0.4s ease-out 0.5s forwards;
  }

  @keyframes drawCircle { to { stroke-dashoffset: 0; } }
  @keyframes drawCheck { to { stroke-dashoffset: 0; } }

  .complete-title {
    font-family: var(--font-display);
    font-size: var(--font-3xl);
    font-weight: var(--weight-regular);
    letter-spacing: -0.02em;
    line-height: var(--leading-tight);
    color: var(--text);
    margin: 0;
  }

  .complete-filename {
    font-size: var(--font-md);
    color: var(--text-secondary);
    margin: 0;
    background: var(--surface);
    padding: var(--space-3) var(--space-8);
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .complete-stats {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    font-size: var(--font-base);
    color: var(--text-secondary);
  }

  .complete-stat-sep { opacity: var(--opacity-faint); }

  .complete-actions {
    display: flex;
    gap: var(--space-4);
    width: 100%;
    max-width: 420px;
  }

  @media (prefers-reduced-motion: reduce) {
    .complete-icon {
      animation: none;
      transform: scale(1);
    }

    .complete-icon :global(.check-circle),
    .complete-icon :global(.check-path) {
      animation: none;
      stroke-dashoffset: 0;
    }
  }
</style>
