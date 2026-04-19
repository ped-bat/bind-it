<script>
  import { slide } from "svelte/transition";
  import Panel from "$lib/components/ui/Panel.svelte";
  import FormField from "$lib/components/ui/FormField.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { settingsStore, FORBIDDEN_FILENAME_CHARS } from "$lib/stores/settings.svelte.js";
  import { browseFolder } from "$lib/services/tauri.js";

  let browsePending = $state(false);

  async function handleBrowse() {
    if (browsePending) return;
    browsePending = true;
    try {
      const dir = await browseFolder();
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
          <input class="u-input u-input--sm" type="text" bind:value={settingsStore.outputDir} placeholder="Output folder" />
          <Button variant="secondary" size="md" disabled={browsePending} onclick={handleBrowse}>
            {#snippet children()}Browse{/snippet}
          </Button>
        </div>
      {/snippet}
    </FormField>
    <FormField label="Filename">
      {#snippet children()}
        <div class="filename-input">
          <input class="u-input u-input--sm filename-input-field" type="text" bind:value={settingsStore.outputFilename} placeholder="output" aria-invalid={invalidChars !== ""} />
          <span class="ext">.m4b</span>
        </div>
        {#if invalidChars}
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
  }

  .filename-warning {
    margin: var(--space-3) 0 0 0;
    font-size: var(--font-sm);
    color: var(--text-secondary);
    line-height: var(--leading-normal);
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
