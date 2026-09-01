<script>
  import LottieMark from "$lib/components/ui/LottieMark.svelte";

  /** @type {{ animation?: 'intro' | 'pulse', loop?: boolean, label?: string }} */
  let { animation = "pulse", loop = false, label = "Bind it" } = $props();
</script>

<header class="app-header">
  <LottieMark {animation} {loop} width={100} height={80} ariaLabel={label} />
  <h1>{label}</h1>
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
    margin: -5px 0 0;
    font-size: var(--font-3xl);
    letter-spacing: -0.02em;
    line-height: var(--leading-tight);
    color: var(--text);
  }
</style>
