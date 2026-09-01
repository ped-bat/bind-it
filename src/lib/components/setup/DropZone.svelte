<script>
  import { fade } from "svelte/transition";
  import AppHeader from "$lib/components/ui/AppHeader.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { addFilesFromBrowse, addFilesFromFolder } from "$lib/services/actions.js";

  let stillProbing = $state(false);

  $effect(() => {
    if (fileStore.probing && fileStore.count === 0) {
      stillProbing = false;
      const timer = setTimeout(() => { stillProbing = true; }, 10000);
      return () => clearTimeout(timer);
    }
    stillProbing = false;
  });
</script>

{#if fileStore.probing && fileStore.count === 0}
  <div class="drop-zone probing-state">
    <div class="spinner"></div>
    <p class="drop-title">Reading files...</p>
    <p class="drop-subtitle">
      {stillProbing ? "Still probing — this can take a moment for large files" : "Probing audio metadata"}
    </p>
  </div>
{:else}
  <!-- Hover state comes from Tauri's native drag-drop events (see
       setupListeners) — DOM drag events don't fire on Windows/Linux
       webviews when native drag-drop is enabled. -->
  <div
    class="drop-zone"
    in:fade={{ duration: 200, delay: 100 }}
    class:drag-over={appStore.dragOver}
    role="region"
    aria-label="Drop audio files or folders here"
  >
    <AppHeader animation="intro" />
    <p class="drop-title drop-title-spaced">Drop files or folders here</p>
    <p class="drop-subtitle">MP3, M4A, M4B, AAC, WAV, FLAC, WMA</p>
    <div class="drop-buttons">
      <Button variant="primary" size="lg" onclick={addFilesFromBrowse}>
        {#snippet children()}Add files{/snippet}
      </Button>
      <Button variant="primary" size="lg" onclick={addFilesFromFolder}>
        {#snippet children()}Add folder{/snippet}
      </Button>
    </div>
  </div>
{/if}

<style>
  .drop-zone {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--border);
    border-radius: var(--radius-lg);
    transition:
      border-color var(--transition),
      background var(--transition);
    gap: var(--space-4);
    min-height: 240px;
  }

  .drop-zone.drag-over {
    border-color: var(--accent);
    border-style: solid;
    background: var(--accent-ghost-8);
  }

  .drop-title {
    font-size: var(--font-xl);
    font-weight: var(--weight-medium);
    margin: 0;
    color: var(--text);
  }

  .drop-title-spaced { margin-top: var(--space-12); }

  .drop-subtitle {
    font-size: var(--font-md);
    color: var(--text-secondary);
    margin: 0;
  }

  .drop-buttons {
    display: flex;
    gap: var(--space-4);
    margin-top: var(--space-8);
  }

  .spinner {
    width: var(--icon-lg);
    height: var(--icon-lg);
    border: 2.5px solid var(--border);
    border-top-color: var(--accent);
    border-radius: var(--radius-full);
    animation: spin 0.8s linear infinite;
  }
</style>
