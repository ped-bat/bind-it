<script>
  import Banner from "$lib/components/ui/Banner.svelte";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { conversionStore } from "$lib/stores/conversion.svelte.js";

  const retryKeywords = ["disk space", "Permission denied", "moved or deleted"];
  const hasRetry = (/** @type {string} */ msg) => retryKeywords.some((k) => msg.includes(k));

  function handleRetry() {
    appStore.dismissError();
    conversionStore.start();
  }
</script>

{#if appStore.error}
  <Banner
    variant="error"
    message={appStore.error}
    retry={hasRetry(appStore.error)}
    dismissing={appStore.dismissingError}
    dismissTitle="Dismiss (Esc)"
    dismissLabel="Dismiss error"
    ondismiss={() => appStore.dismissError()}
    onretry={hasRetry(appStore.error) ? handleRetry : undefined}
  />
{/if}

{#if appStore.warning}
  <Banner
    variant="warning"
    message={appStore.warning}
    dismissLabel="Dismiss warning"
    ondismiss={() => appStore.dismissWarning()}
  />
{/if}
