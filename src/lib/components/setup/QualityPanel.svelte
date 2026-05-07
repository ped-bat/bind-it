<script>
  import { slide } from "svelte/transition";
  import Panel from "$lib/components/ui/Panel.svelte";
  import FormField from "$lib/components/ui/FormField.svelte";
  import FieldLabel from "$lib/components/ui/FieldLabel.svelte";
  import SegmentedButtonGroup from "$lib/components/ui/SegmentedButtonGroup.svelte";
  import Banner from "$lib/components/ui/Banner.svelte";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { settingsStore } from "$lib/stores/settings.svelte.js";
  import { formatDurationHuman, estimateSize } from "$lib/services/format.js";

  const LOSSY_CODECS = ["mp3", "wma"];
  const NATIVE_M4B_CODECS = ["aac", "mp3", "alac"];
  const MODE_OPTIONS = [
    { value: "lossless", label: "Lossless" },
    { value: "compress", label: "Compress" },
  ];
  const CHANNEL_OPTIONS = [
    { value: false, label: "Stereo" },
    { value: true, label: "Mono" },
  ];
  const FORMAT_OPTIONS = [
    { value: "original", label: "Original (preserve source codec)" },
    { value: "original-m4b", label: "Original wrapped in M4B" },
    { value: "aac", label: "AAC (lossy, M4B)" },
    { value: "alac", label: "ALAC (lossless, M4B)" },
  ];

  $effect(() => {
    void settingsStore.bitrate, settingsStore.mono, settingsStore.qualityMode;
    settingsStore.persistQuality();
  });

  const isLossless = $derived(settingsStore.qualityMode === "lossless");
  const format = $derived(settingsStore.outputFormat);
  // AAC pins quality to compress; ALAC pins quality to lossless. Surface
  // that in the segmented control by disabling the unavailable option.
  const lossyOnly = $derived(format === "aac");
  const losslessOnly = $derived(format === "alac");
  const modeOptions = $derived(MODE_OPTIONS.map(o => ({
    ...o,
    disabled: (o.value === "lossless" && lossyOnly) || (o.value === "compress" && losslessOnly),
  })));

  const allSameNative = $derived(
    fileStore.items.length > 0
      && NATIVE_M4B_CODECS.includes(fileStore.items[0].codec)
      && fileStore.items.every(f => f.codec === fileStore.items[0].codec)
  );
  const willOutputAlac = $derived(format === "alac"
    || (format === "original" && isLossless && !allSameNative)
    || (format === "original-m4b" && isLossless && !allSameNative));
  const hasLossySource = $derived(fileStore.items.some(f => LOSSY_CODECS.includes(f.codec)));

  /** @param {Event} e */
  function onFormatChange(e) {
    const v = /** @type {HTMLSelectElement} */ (e.currentTarget).value;
    settingsStore.setOutputFormat(/** @type {any} */ (v));
  }

  function effectiveBps() {
    if (isLossless) {
      return fileStore.avgBitrate || settingsStore.bitrate * 1000;
    }
    const channels = settingsStore.mono ? 1 : 2;
    return settingsStore.bitrate * 1000 * channels;
  }
</script>

{#if fileStore.mergePlan}
  <Panel title="Quality">
    <div class="encoding-fields">
      <div class="quality-row">
        <FormField label="Output format">
          {#snippet children()}
            <select class="u-input u-input--sm" value={format} onchange={onFormatChange}>
              {#each FORMAT_OPTIONS as opt}
                <option value={opt.value}>{opt.label}</option>
              {/each}
            </select>
          {/snippet}
        </FormField>
      </div>

      <div class="quality-row">
        <FieldLabel>{#snippet children()}Mode{/snippet}</FieldLabel>
        <SegmentedButtonGroup bind:value={settingsStore.qualityMode} options={modeOptions} ariaLabel="Quality mode" />
      </div>

      {#if !isLossless}
        <div class="quality-row" transition:slide={{ duration: 180 }}>
          <FieldLabel>{#snippet children()}Channels{/snippet}</FieldLabel>
          <SegmentedButtonGroup bind:value={settingsStore.mono} options={CHANNEL_OPTIONS} ariaLabel="Channels" />
        </div>
        <div class="quality-row" transition:slide={{ duration: 180 }}>
          <FormField label="Bitrate per channel">
            {#snippet children()}
              <select class="u-input u-input--sm" bind:value={settingsStore.bitrate}>
                <option value={64}>64 kbps</option>
                <option value={96}>96 kbps</option>
                <option value={128}>128 kbps</option>
                <option value={192}>192 kbps</option>
                <option value={256}>256 kbps</option>
                <option value={320}>320 kbps</option>
              </select>
            {/snippet}
          </FormField>
        </div>
      {/if}

      {#if willOutputAlac && hasLossySource}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="info"
            dismissible={false}
            message="Some files are already compressed (MP3/WMA). Storing them losslessly preserves current quality but produces a larger file."
          />
        </div>
      {/if}

      {#if willOutputAlac}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="info"
            dismissible={false}
            message="ALAC M4B plays on Apple devices; some third-party players may not open it."
          />
        </div>
      {/if}

      <div class="quality-row">
        <FieldLabel>{#snippet children()}Expected output{/snippet}</FieldLabel>
        <div class="expected-output">
          <span>{fileStore.count} file{fileStore.count !== 1 ? "s" : ""}</span>
          <span class="expected-sep">&middot;</span>
          <span>{formatDurationHuman(fileStore.totalDuration)}</span>
          {#if !willOutputAlac}
            <span class="expected-sep">&middot;</span>
            <span>{estimateSize(fileStore.totalDuration, effectiveBps())}</span>
          {/if}
        </div>
      </div>
    </div>
  </Panel>
{/if}

<style>
  .encoding-fields {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }

  .quality-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }

  .expected-output {
    font-size: var(--font-base);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .expected-sep {
    margin: 0 var(--space-2);
    color: var(--text-secondary);
    opacity: var(--opacity-faint);
  }
</style>
