<script>
  import LottieMark from "$lib/components/ui/LottieMark.svelte";

  /** @type {{ size?: 'sm' | 'md' | 'lg', animation?: 'intro' | 'pulse', loop?: boolean, label?: string }} */
  let { size = "md", animation = "pulse", loop = false, label = "Bind it" } = $props();

  const dims = $derived.by(() => {
    return { logoW: 100, logoH: 80, font: "var(--font-3xl)" };
  });
</script>

<header class="app-header">
  <LottieMark {animation} {loop} width={dims.logoW} height={dims.logoH} ariaLabel={label} />
  <h1 style="margin-top:-5px; font-size: {dims.font};">{label}</h1>
</header>

<style>
  .app-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
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
