<script>
  import { slide } from "svelte/transition";
  import Panel from "$lib/components/ui/Panel.svelte";
  import FormField from "$lib/components/ui/FormField.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { settingsStore, FORBIDDEN_FILENAME_CHARS } from "$lib/stores/settings.svelte.js";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { browseFolder } from "$lib/services/tauri.js";
  import { outputExtension } from "$lib/services/output.js";

  const ext = $derived(outputExtension());

  const folderEmpty = $derived(!settingsStore.outputDir.trim());
  const filenameEmpty = $derived(!settingsStore.outputFilename.trim());
  const showFolderError = $derived(appStore.validationAttempted && folderEmpty);
  const showFilenameError = $derived(appStore.validationAttempted && filenameEmpty);

  let browsePending = $state(false);

  async function handleBrowse() {
    if (browsePending) return;
    browsePending = true;
    try {
      const dir = await browseFolder(settingsStore.outputDir);
      if (dir) {
        settingsStore.outputDir = dir;
        settingsStore.persistOutputDir();
      }
    } finally {
      browsePending = false;
    }
  }

  $effect(() => {
    void settingsStore.outputDir;
    settingsStore.persistOutputDir();
  });

  const invalidChars = $derived.by(() => {
    const matches = settingsStore.outputFilename.match(FORBIDDEN_FILENAME_CHARS);
    if (!matches) return "";
    return [...new Set(matches)].join(" ");
  });
</script>

<Panel title="Output">
  <div class="output-fields">
    <FormField label="Folder">
      {#snippet children()}
        <div class="dir-input">
          <input
            class="u-input u-input--sm"
            type="text"
            bind:value={settingsStore.outputDir}
            placeholder="Output folder"
            aria-invalid={showFolderError}
          />
          <Button variant="secondary" size="md" disabled={browsePending} onclick={handleBrowse}>
            {#snippet children()}Browse{/snippet}
          </Button>
        </div>
        {#if showFolderError}
          <p class="field-error" transition:slide={{ duration: 150 }}>Please fill the output folder.</p>
        {/if}
      {/snippet}
    </FormField>
    <FormField label="Filename">
      {#snippet children()}
        <div class="filename-input" class:invalid={showFilenameError}>
          <input
            class="u-input u-input--sm filename-input-field"
            type="text"
            bind:value={settingsStore.outputFilename}
            placeholder="output"
            aria-invalid={invalidChars !== "" || showFilenameError}
          />
          <span class="ext">.{ext}</span>
        </div>
        {#if showFilenameError}
          <p class="field-error" transition:slide={{ duration: 150 }}>Please fill the filename.</p>
        {:else if invalidChars}
          <p class="filename-warning" transition:slide={{ duration: 150 }}>
            These characters will be removed: <code>{invalidChars}</code>
          </p>
        {/if}
      {/snippet}
    </FormField>
  </div>
</Panel>

<style>
  .output-fields {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .dir-input {
    display: flex;
    gap: var(--space-3);
  }

  .dir-input input { flex: 1; }

  .filename-input {
    display: flex;
    align-items: stretch;
  }

  .filename-input-field {
    flex: 1;
    border-radius: var(--radius-md) 0 0 var(--radius-md);
    border-right: none;
  }

  .filename-input .ext {
    color: var(--text-secondary);
    font-size: var(--font-md);
    padding: 0 var(--space-5);
    background: var(--bg);
    border: 1px solid var(--border);
    border-left: none;
    border-radius: 0 var(--radius-md) var(--radius-md) 0;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    height: var(--control-md);
    box-sizing: border-box;
    transition: border-color var(--transition);
  }

  /* Mirror the input's hover/focus state on the trailing extension chip so
     the joined control reads as a single field. */
  .filename-input:hover .ext { border-color: var(--accent-border-mix); }

  .filename-input:focus-within .ext {
    border-color: var(--accent);
  }

  .filename-input:focus-within {
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-focus-strong);
  }

  .filename-input:focus-within .filename-input-field {
    box-shadow: none;
  }

  .filename-warning {
    margin: var(--space-3) 0 0 0;
    font-size: var(--font-sm);
    color: var(--text-secondary);
    line-height: var(--leading-normal);
  }

  .field-error {
    margin: var(--space-3) 0 0 0;
    font-size: var(--font-sm);
    color: var(--error);
    line-height: var(--leading-normal);
  }

  /* Invalid state: red border on the input itself, plus on the joined ext
     chip when the filename field is invalid. Drives off aria-invalid so
     accessibility and visuals stay in sync. */
  :global(.u-input[aria-invalid="true"]) {
    border-color: var(--error);
  }

  :global(.u-input[aria-invalid="true"]:focus),
  :global(.u-input[aria-invalid="true"]:focus-visible) {
    border-color: var(--error);
    box-shadow: 0 0 0 3px var(--error-ghost-30);
  }

  .filename-input.invalid .ext {
    border-color: var(--error);
  }

  .filename-input.invalid:focus-within {
    box-shadow: 0 0 0 3px var(--error-ghost-30);
  }

  .filename-warning code {
    color: var(--text);
    background: var(--accent-ghost-8);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--font-xs);
  }
</style>
