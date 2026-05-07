<script>
  import { fade } from "svelte/transition";
  import { tick } from "svelte";
  import AppHeader from "$lib/components/ui/AppHeader.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { conversionStore } from "$lib/stores/conversion.svelte.js";
  import { formatElapsed } from "$lib/services/format.js";

  /** @type {HTMLDivElement | undefined} */
  let actions;

  $effect(() => {
    tick().then(() => {
      const btn = actions?.querySelector("button");
      if (btn instanceof HTMLButtonElement) btn.focus();
    });
  });
</script>

<div class="converting-screen" in:fade={{ duration: 300 }}>
  <header class="converting-header">
    <AppHeader size="sm" animation="pulse" loop label="Binding" />
  </header>

  <div class="converting-content">
    <div class="converting-progress">
      <p class="converting-percent">
        <span class="percent-value">
          {#each [...String(conversionStore.displayPercent)] as digit, i (i)}
            <span class="digit">{digit}</span>
          {/each}
        </span><span class="digit percent-sign">%</span>
      </p>
      <div class="converting-bar-track" role="progressbar" aria-valuenow={conversionStore.displayPercent} aria-valuemin={0} aria-valuemax={100} aria-label="Conversion progress">
        <div class="converting-bar-fill" style="width: {conversionStore.displayPercent}%"></div>
        <div class="converting-bar-shine-clip" aria-hidden="true" style="clip-path: inset(0 {100 - conversionStore.displayPercent}% 0 0 round var(--radius-pill))">
          <div class="converting-bar-shine"></div>
        </div>
      </div>
      <p class="converting-message">{conversionStore.displayMessage}</p>
      <p class="converting-elapsed">Elapsed time: {formatElapsed(conversionStore.elapsedSeconds)}</p>
    </div>

    <div bind:this={actions}>
      <Button variant="secondary" size="lg" ariaLabel="Cancel conversion" onclick={() => conversionStore.cancel()}>
        {#snippet children()}Cancel{/snippet}
      </Button>
    </div>
  </div>
</div>

<style>
  .converting-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .converting-header {
    display: flex;
    justify-content: center;
  }

  .converting-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-16);
  }

  .converting-progress {
    width: 100%;
    max-width: 480px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-6);
  }

  .converting-percent {
    font-family: var(--font-display);
    font-size: var(--font-display-hero);
    font-weight: var(--weight-regular);
    font-variant-numeric: tabular-nums;
    font-feature-settings: "tnum";
    color: var(--text);
    margin: 0;
    letter-spacing: -0.02em;
    line-height: var(--leading-tight);
    min-width: 4.5ch;
    text-align: center;
  }

  .percent-value {
    display: inline-block;
    transition: transform 0.3s ease-out;
  }

  .digit {
    display: inline-block;
    width: 0.45em;
    text-align: center;
  }

  .percent-sign {
    width: auto;
    margin-left: 0.05em;
    color: var(--text-secondary);
  }

  .converting-bar-track {
    width: 100%;
    height: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    overflow: hidden;
    position: relative;
  }

  .converting-bar-fill {
    height: 100%;
    background: var(--accent);
    border-radius: var(--radius-pill);
    transition: width 0.5s ease-out;
  }

  .converting-bar-shine-clip {
    position: absolute;
    inset: 0;
    pointer-events: none;
    transition: clip-path 0.5s ease-out;
  }

  .converting-bar-shine {
    position: absolute;
    top: 0;
    height: 100%;
    width: 30%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgba(255, 255, 255, 0.5) 50%,
      transparent 100%
    );
    animation: barSweep 2s linear infinite;
    will-change: left;
  }

  @keyframes barSweep {
    0%   { left: -30%; }
    100% { left: 100%; }
  }

  .converting-message {
    font-size: var(--font-md);
    color: var(--text-secondary);
    margin: 0;
    text-align: center;
    font-variant-numeric: tabular-nums;
    font-feature-settings: "tnum";
  }

  .converting-elapsed {
    font-size: var(--font-md);
    color: var(--text-secondary);
    opacity: var(--opacity-subtle);
    margin: calc(var(--space-2) * -1) 0 0 0;
    font-variant-numeric: tabular-nums;
    font-feature-settings: "tnum";
  }

  @media (prefers-reduced-motion: reduce) {
    .converting-bar-shine { animation: none; }
  }
</style>
