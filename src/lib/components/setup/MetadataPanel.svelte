<script>
  import Panel from "$lib/components/ui/Panel.svelte";
  import FormField from "$lib/components/ui/FormField.svelte";
  import IconButton from "$lib/components/ui/IconButton.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { metadataStore } from "$lib/stores/metadata.svelte.js";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { browseImage, setCustomCoverArt } from "$lib/services/tauri.js";

  let coverPending = $state(false);

  async function handleChooseCoverArt() {
    if (coverPending) return;
    coverPending = true;
    try {
      const path = await browseImage();
      if (path) {
        const art = await setCustomCoverArt(path);
        fileStore.coverArt = art.data_uri;
        fileStore.coverArtPath = art.file_path;
      }
    } catch (e) {
      appStore.error = String(e);
    } finally {
      coverPending = false;
    }
  }

  function clearCover(/** @type {MouseEvent} */ e) {
    e.stopPropagation();
    fileStore.coverArt = null;
    fileStore.coverArtPath = null;
  }
</script>

<Panel title="Metadata">
  <div class="metadata-content">
    <div class="cover-art-container">
      {#if fileStore.coverArt}
        <button class="cover-art-btn" onclick={handleChooseCoverArt} disabled={coverPending} aria-label="Change cover art">
          <img class="cover-art" src={fileStore.coverArt} alt="Cover art" />
        </button>
        <div class="cover-remove">
          <IconButton
            variant="danger"
            size="sm"
            shape="circle"
            onclick={clearCover}
            title="Remove cover art"
            ariaLabel="Remove cover art"
          >
            {#snippet children()}&times;{/snippet}
          </IconButton>
        </div>
      {:else}
        <button class="cover-art-btn cover-placeholder" onclick={handleChooseCoverArt} disabled={coverPending} aria-label="Choose cover art (no cover detected — click to choose image)" title="No cover detected — click to choose image">
          <Icon name="image-placeholder" width={32} height={32} />
        </button>
      {/if}
    </div>
    <div class="metadata-fields">
      <FormField label="Title">
        {#snippet children()}
          <input class="u-input u-input--sm" type="text" bind:value={metadataStore.title} placeholder="Audiobook title" />
        {/snippet}
      </FormField>
      <FormField label="Author">
        {#snippet children()}
          <input class="u-input u-input--sm" type="text" bind:value={metadataStore.artist} placeholder="Author name" />
        {/snippet}
      </FormField>
      <FormField label="Album">
        {#snippet children()}
          <input class="u-input u-input--sm" type="text" bind:value={metadataStore.album} placeholder="Album / Series" />
        {/snippet}
      </FormField>
      <FormField label="Narrator">
        {#snippet children()}
          <input class="u-input u-input--sm" type="text" bind:value={metadataStore.narrator} placeholder="Narrator name" />
        {/snippet}
      </FormField>
      <FormField label="Year">
        {#snippet children()}
          <input class="u-input u-input--sm" type="text" bind:value={metadataStore.year} placeholder="Year" />
        {/snippet}
      </FormField>
    </div>
  </div>
</Panel>

<style>
  .metadata-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-8);
  }

  .cover-art-container {
    position: relative;
    flex-shrink: 0;
    display: inline-flex;
    align-self: center;
  }

  .cover-remove {
    position: absolute;
    top: -10px;
    right: -10px;
    z-index: 2;
  }

  .cover-remove :global(.icon-btn) {
    background: var(--bg);
    border: 1px solid var(--border);
  }

  .cover-art-btn {
    position: relative;
    padding: 0;
    margin: 0;
    background: none;
    border: none;
    cursor: pointer;
    flex-shrink: 0;
  }

  .cover-art {
    width: 120px;
    height: 120px;
    border-radius: var(--radius-lg);
    object-fit: cover;
    display: block;
    border: 1px solid var(--border);
    transition: border-color var(--transition), box-shadow var(--transition);
  }

  .cover-art-btn:hover .cover-art {
    border-color: var(--accent);
    box-shadow: var(--shadow-focus-strong);
  }

  .cover-placeholder {
    width: 120px;
    height: 120px;
    border-radius: var(--radius-lg);
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    opacity: var(--opacity-muted);
    flex-shrink: 0;
    cursor: pointer;
    transition: border-color var(--transition), opacity var(--transition);
  }

  .cover-placeholder:hover {
    border-color: var(--accent);
    opacity: var(--opacity-strong);
  }

  .metadata-fields {
    flex: 1;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    min-width: 0;
  }

  .cover-art-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: var(--radius-lg);
  }
</style>
