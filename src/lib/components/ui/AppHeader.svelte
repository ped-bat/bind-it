<script>
  import LogoMark from "$lib/LogoMark.svelte";

  /** @type {{ size?: 'sm' | 'md' | 'lg' }} */
  let { size = "md" } = $props();

  const dims = $derived.by(() => {
    if (size === "sm") return { logoW: 30, logoH: 26, font: "var(--font-2xl)" };
    if (size === "lg") return { logoW: 76, logoH: 64, font: "var(--font-4xl)" };
    return { logoW: 60, logoH: 50, font: "var(--font-3xl)" };
  });
</script>

<header class="app-header">
  <LogoMark width={dims.logoW} height={dims.logoH} />
  <h1 style="font-size: {dims.font};">Bindery</h1>
</header>

<style>
  .app-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
  }

  .app-header :global(.logo-mark) {
    color: var(--accent);
  }

  h1 {
    font-family: var(--font-display);
    font-weight: var(--weight-regular);
    margin: 0;
    letter-spacing: -0.02em;
    line-height: var(--leading-tight);
    color: var(--text);
  }
</style>
