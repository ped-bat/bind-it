<script>
  import AppHeader from "$lib/components/ui/AppHeader.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import ErrorBanner from "$lib/components/ui/ErrorBanner.svelte";
  import DropZone from "$lib/components/setup/DropZone.svelte";
  import FileList from "$lib/components/setup/FileList.svelte";
  import MetadataPanel from "$lib/components/setup/MetadataPanel.svelte";
  import OutputPanel from "$lib/components/setup/OutputPanel.svelte";
  import QualityPanel from "$lib/components/setup/QualityPanel.svelte";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { conversionStore } from "$lib/stores/conversion.svelte.js";
  import { confirmAsk } from "$lib/services/tauri.js";

  async function handleCancel() {
    if (fileStore.count > 0) {
      const ok = await confirmAsk(
        `Discard ${fileStore.count} chapter${fileStore.count !== 1 ? "s" : ""} and metadata?`,
        { title: "Discard changes", okLabel: "Discard", cancelLabel: "Keep" },
      );
      if (!ok) return;
    }
    appStore.clearAll();
  }
</script>

<ErrorBanner />

{#if fileStore.count === 0}
  <DropZone />
{:else}
  <div class="content">
    <AppHeader size="sm" />

    <FileList />

    <div class="two-col">
      <MetadataPanel />
      <div class="right-col">
        <OutputPanel />
        <QualityPanel />
      </div>
    </div>
  </div>

  <div class="convert-section">
    <Button variant="secondary" size="lg" flex onclick={handleCancel}>
      {#snippet children()}Cancel{/snippet}
    </Button>
    <Button
      variant="primary"
      size="lg"
      flex
      disabled={fileStore.count < 1 || !appStore.ffmpegOk}
      onclick={() => conversionStore.start()}
    >
      {#snippet children()}Bind it!{/snippet}
    </Button>
  </div>
{/if}

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-12);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: var(--space-6);
    margin-right: calc(var(--space-6) * -1);
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-12);
    align-items: start;
  }

  @media (max-width: 640px) {
    .two-col { grid-template-columns: 1fr; }
  }

  .right-col {
    display: flex;
    flex-direction: column;
    gap: var(--space-12);
  }

  .convert-section {
    display: flex;
    gap: var(--space-4);
    flex-shrink: 0;
  }
</style>
