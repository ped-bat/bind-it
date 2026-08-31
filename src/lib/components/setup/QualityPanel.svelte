<script>
  import { slide } from "svelte/transition";
  import Panel from "$lib/components/ui/Panel.svelte";
  import FormField from "$lib/components/ui/FormField.svelte";
  import FieldLabel from "$lib/components/ui/FieldLabel.svelte";
  import SegmentedButtonGroup from "$lib/components/ui/SegmentedButtonGroup.svelte";
  import Banner from "$lib/components/ui/Banner.svelte";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { settingsStore } from "$lib/stores/settings.svelte.js";
  import { formatDurationHuman, estimateSize, formatBytes } from "$lib/services/format.js";
  import { outputContainerLabel, outputCodecLabel } from "$lib/services/output.js";

  const LOSSY_CODECS = ["mp3", "wma"];
  const ALWAYS_TRANSCODE_CODECS = ["flac", "wav", "wma"];
  const MODE_OPTIONS = [
    { value: "lossless", label: "Lossless" },
    { value: "compress", label: "Compress" },
  ];
  const CHANNEL_OPTIONS = [
    { value: false, label: "Stereo" },
    { value: true, label: "Mono" },
  ];
  const MP3_FORMAT_OPTIONS = [
    { value: "mp3", label: "MP3" },
    { value: "mp3-m4b", label: "MP3 (wrapped in M4B)" },
    { value: "reencode", label: "Re-encode" },
  ];

  $effect(() => {
    void settingsStore.bitrate, settingsStore.mono, settingsStore.qualityMode;
    settingsStore.persistQuality();
  });

  const items = $derived(fileStore.items);
  const allMp3 = $derived(items.length > 0 && items.every(f => f.codec === "mp3"));

  // Distinct codec count drives both the "preserve possible?" check and the
  // mixed-codec banner. WMA / FLAC / WAV always re-encode regardless.
  const codecSet = $derived(new Set(items.map(f => f.codec)));
  const hasAlwaysTranscode = $derived(items.some(f => ALWAYS_TRANSCODE_CODECS.includes(f.codec)));
  const isMixedCodecs = $derived(codecSet.size > 1);

  // SR/CH uniformity. "Non-uniform" is only meaningful for paths where a
  // bitstream-copy of the majority is possible — the table in
  // docs/output-format-rules.md scopes the warning accordingly.
  const srChCounts = $derived.by(() => {
    /** @type {Map<string, number>} */
    const counts = new Map();
    for (const f of items) {
      const k = `${f.sample_rate}|${f.channels}`;
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    return counts;
  });
  const isUniformSrCh = $derived(srChCounts.size <= 1);
  const outlierCount = $derived.by(() => {
    if (srChCounts.size <= 1) return 0;
    let max = 0;
    for (const c of srChCounts.values()) if (c > max) max = c;
    return items.length - max;
  });

  const mp3Choice = $derived(settingsStore.mp3FormatChoice);
  const isMp3CopyPath = $derived(allMp3 && (mp3Choice === "mp3" || mp3Choice === "mp3-m4b"));
  const isLossless = $derived(settingsStore.qualityMode === "lossless");

  // Mode toggle is locked to Lossless when the user picked an MP3 bitstream-copy
  // path — both options preserve the source frames so re-encoding is irrelevant.
  const modeOptions = $derived(MODE_OPTIONS.map(o => ({
    ...o,
    disabled: isMp3CopyPath,
  })));

  $effect(() => {
    if (isMp3CopyPath && settingsStore.qualityMode !== "lossless") {
      settingsStore.qualityMode = "lossless";
    }
  });

  // Output codec the binding will produce for the chosen UI state. Drives
  // banner visibility for ALAC-specific warnings.
  const willOutputAlac = $derived.by(() => {
    if (allMp3) {
      // MP3 input: only the Re-encode → Lossless combo lands on ALAC.
      return mp3Choice === "reencode" && isLossless;
    }
    // Non-MP3 input: Lossless mode lands on ALAC iff source isn't already
    // a preservable codec (aac/alac). Otherwise stays as remux/copy.
    if (!isLossless) return false;
    return hasAlwaysTranscode || isMixedCodecs;
  });

  const willOutputAac = $derived.by(() => {
    if (allMp3) {
      return mp3Choice === "reencode" && !isLossless;
    }
    return !isLossless; // Compress mode → AAC for any non-MP3 input
  });

  // The non-uniform banner only fires when the path mixes copy + re-encode.
  // That is: aac/alac/mp3 input with their respective preserve selections.
  const showNonUniformBanner = $derived.by(() => {
    if (isUniformSrCh) return false;
    if (allMp3 && (mp3Choice === "mp3" || mp3Choice === "mp3-m4b")) return true;
    if (!allMp3 && isLossless && !hasAlwaysTranscode && !isMixedCodecs) {
      // Uniform-codec aac or alac, Lossless mode → backend re-encodes outliers
      return true;
    }
    return false;
  });

  // Generation loss vs lossless wording differs by codec.
  const nonUniformIsLossy = $derived.by(() => {
    if (allMp3) return true; // MP3 outliers re-encoded → generation loss
    const onlyAlac = codecSet.size === 1 && codecSet.has("alac");
    return !onlyAlac; // AAC outliers → generation loss; ALAC outliers stay lossless
  });

  // Apple Books quirks banner — fires for any MP3-derivative output.
  const showMp3PlainBanner = $derived(allMp3 && mp3Choice === "mp3");
  const showMp3M4bBanner = $derived(allMp3 && mp3Choice === "mp3-m4b");

  // ALAC banners fire when we'll produce ALAC output.
  const hasLossySource = $derived(items.some(f => LOSSY_CODECS.includes(f.codec)));
  const showAlacLossySourceBanner = $derived(willOutputAlac && hasLossySource);
  const showAlacPlayerBanner = $derived(willOutputAlac);

  const showMixedCodecBanner = $derived(isMixedCodecs);
  const codecListLabel = $derived([...codecSet].sort().join(", "));

  /** @param {Event} e */
  function onMp3ChoiceChange(e) {
    const v = /** @type {HTMLSelectElement} */ (e.currentTarget).value;
    settingsStore.setMp3FormatChoice(/** @type {any} */ (v));
  }

  function effectiveBps() {
    if (isLossless) {
      return fileStore.avgBitrate || settingsStore.bitrate * 1000;
    }
    const channels = settingsStore.mono ? 1 : 2;
    return settingsStore.bitrate * 1000 * channels;
  }

  // Output-size estimate. Three branches:
  //   - MP3 copy path: bitstream-copy ≈ sum of source file sizes (header
  //     stripping + container overhead is negligible).
  //   - ALAC: variable-rate lossless. Rough estimate via 16-bit PCM at the
  //     majority sample-rate / channel layout, scaled by a typical ALAC
  //     compression ratio (~0.55 for spoken-word / mixed content).
  //   - Everything else: derive from chosen bitrate × duration.
  const sizeEstimate = $derived.by(() => {
    if (isMp3CopyPath) {
      const total = items.reduce((s, f) => s + (f.file_size || 0), 0);
      if (total <= 0) return null;
      return `~${formatBytes(total)}`;
    }
    if (willOutputAlac) {
      const dur = fileStore.totalDuration;
      if (dur <= 0) return null;
      const sr = mostCommon(items.map(f => f.sample_rate), 44100);
      const ch = mostCommon(items.map(f => f.channels), 2);
      const ALAC_RATIO = 0.55;
      const bps = sr * ch * 16 * ALAC_RATIO;
      return estimateSize(dur, bps);
    }
    return estimateSize(fileStore.totalDuration, effectiveBps());
  });

  /**
   * @param {number[]} values
   * @param {number} fallback
   */
  function mostCommon(values, fallback) {
    /** @type {Map<number, number>} */
    const counts = new Map();
    for (const v of values) {
      if (!v) continue;
      counts.set(v, (counts.get(v) ?? 0) + 1);
    }
    let best = fallback;
    let bestCount = 0;
    for (const [v, c] of counts) {
      if (c > bestCount) { best = v; bestCount = c; }
    }
    return best;
  }

  const labelInputs = $derived({
    items,
    mp3Choice,
    qualityMode: settingsStore.qualityMode,
  });
  const containerLabel = $derived(outputContainerLabel(labelInputs));
  const codecLabel = $derived(outputCodecLabel(labelInputs));
</script>

{#if fileStore.mergePlan}
  <Panel title="Quality">
    <div class="encoding-fields">
      {#if allMp3}
        <div class="quality-row">
          <FormField label="Output format">
            {#snippet children()}
              <select class="u-input u-input--sm" value={mp3Choice} onchange={onMp3ChoiceChange}>
                {#each MP3_FORMAT_OPTIONS as opt}
                  <option value={opt.value}>{opt.label}</option>
                {/each}
              </select>
            {/snippet}
          </FormField>
        </div>
      {/if}

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

      {#if showMp3PlainBanner}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="info"
            dismissible={false}
            message="Output is .mp3 with ID3v2 chapters. Apple Books may render chapter markers incorrectly."
          />
        </div>
      {/if}

      {#if showMp3M4bBanner}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="info"
            dismissible={false}
            message="MP3 stream wrapped in M4B. Apple Books shows chapters correctly; some players (e.g. macOS Preview) may not open it."
          />
        </div>
      {/if}

      {#if showNonUniformBanner}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="warning"
            dismissible={false}
            message={
              `${outlierCount} file${outlierCount !== 1 ? "s" : ""} have a different sample rate or channel count and will be re-encoded to match the rest. ` +
              (nonUniformIsLossy ? "This causes a small quality loss on those files only." : "This stays lossless.")
            }
          />
        </div>
      {/if}

      {#if showMixedCodecBanner}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="info"
            dismissible={false}
            message={`Sources have multiple codecs (${codecListLabel}). All files will be re-encoded to a single codec for the merged output.`}
          />
        </div>
      {/if}

      {#if showAlacLossySourceBanner}
        <div transition:slide={{ duration: 180 }}>
          <Banner
            variant="info"
            dismissible={false}
            message="Some files are already compressed (MP3 / WMA). Storing them losslessly preserves current quality but produces a larger file."
          />
        </div>
      {/if}

      {#if showAlacPlayerBanner}
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
          <span>{containerLabel}</span>
          {#if codecLabel !== containerLabel}
            <span class="expected-sep">&middot;</span>
            <span>{codecLabel}</span>
          {/if}
          <span class="expected-sep">&middot;</span>
          <span>{fileStore.count} file{fileStore.count !== 1 ? "s" : ""}</span>
          <span class="expected-sep">&middot;</span>
          <span>{formatDurationHuman(fileStore.totalDuration)}</span>
          {#if sizeEstimate}
            <span class="expected-sep">&middot;</span>
            <span>{sizeEstimate}</span>
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
    font-size: var(--font-md);
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .expected-sep {
    margin: 0 var(--space-2);
    color: var(--text-secondary);
    opacity: var(--opacity-faint);
  }
</style>
