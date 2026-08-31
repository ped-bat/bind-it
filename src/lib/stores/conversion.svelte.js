import { appStore } from "./app.svelte.js";
import { fileStore } from "./files.svelte.js";
import { metadataStore } from "./metadata.svelte.js";
import { settingsStore } from "./settings.svelte.js";
import { sanitizeFilename } from "./settings.svelte.js";
import { preflightCheck, mergeAudioFiles, cancelMerge, revealInFolder } from "$lib/services/tauri.js";
import { outputExtension, outputContainerLabel, outputCodecLabel } from "$lib/services/output.js";

const ALWAYS_TRANSCODE_CODECS = ["flac", "wav", "wma"];

/**
 * Map UI state (input set + MP3 choice + quality mode) to the backend's
 * (output_codec, force_transcode, wrap_in_mp4) tuple. See
 * docs/output-format-rules.md for the full matrix.
 *
 * @param {{ codec: string }[]} items
 * @param {"mp3" | "mp3-m4b" | "reencode"} mp3Choice
 * @param {"lossless" | "compress"} qualityMode
 */
function backendTuple(items, mp3Choice, qualityMode) {
  const allMp3 = items.length > 0 && items.every(f => f.codec === "mp3");
  const codecSet = new Set(items.map(f => f.codec));
  const hasAlwaysTranscode = items.some(f => ALWAYS_TRANSCODE_CODECS.includes(f.codec));
  const isMixed = codecSet.size > 1;
  const lossless = qualityMode === "lossless";

  if (allMp3) {
    if (mp3Choice === "mp3") return { output_codec: null, force_transcode: false, wrap_in_mp4: false };
    if (mp3Choice === "mp3-m4b") return { output_codec: null, force_transcode: false, wrap_in_mp4: true };
    // reencode
    if (lossless) return { output_codec: "alac", force_transcode: false, wrap_in_mp4: false };
    return { output_codec: "aac", force_transcode: true, wrap_in_mp4: false };
  }

  // Non-MP3 inputs.
  if (!lossless) return { output_codec: "aac", force_transcode: true, wrap_in_mp4: false };

  // Lossless: preserve when source codec is preservable (uniform aac/alac);
  // otherwise full transcode to ALAC.
  if (hasAlwaysTranscode || isMixed) {
    return { output_codec: "alac", force_transcode: false, wrap_in_mp4: false };
  }
  return { output_codec: null, force_transcode: false, wrap_in_mp4: false };
}

/**
 * Mirrors the Rust `Stage` enum (src-tauri/src/lib.rs).
 * @typedef {"preparing" | "transcoding" | "merging" | "chapters" | "done"} Stage
 */
export const STAGES = Object.freeze({
  PREPARING: /** @type {Stage} */ ("preparing"),
  TRANSCODING: /** @type {Stage} */ ("transcoding"),
  MERGING: /** @type {Stage} */ ("merging"),
  CHAPTERS: /** @type {Stage} */ ("chapters"),
  DONE: /** @type {Stage} */ ("done"),
});

class ConversionStore {
  progress = $state({ stage: "", percent: 0, message: "" });
  displayPercent = $state(0);
  displayMessage = $state("");
  elapsedSeconds = $state(0);
  /** @type {string | null} */
  outputPath = $state(null);
  /** @type {{ filename: string, elapsed: number, fileCount: number, totalDuration: number, sizeBytes: number, containerLabel: string, codecLabel: string } | null} */
  completionData = $state(null);

  /** @type {ReturnType<typeof setInterval> | null} */
  #timer = null;

  // Stats snapshotted when the merge starts, so files added while a
  // conversion runs can't inflate the completion summary.
  /** @type {{ fileCount: number, totalDuration: number } | null} */
  #startStats = null;

  // Merge events can arrive after a cancel or after the user is back on the
  // setup screen (e.g. a drop error kicked them there). Only a conversion
  // that is actually on screen may react to them.
  #eventsExpected() {
    return appStore.screen === "converting";
  }

  #startTimer() {
    this.elapsedSeconds = 0;
    this.#timer = setInterval(() => { this.elapsedSeconds += 1; }, 1000);
  }

  #stopTimer() {
    if (this.#timer) { clearInterval(this.#timer); this.#timer = null; }
  }

  /** @param {any} payload */
  handleProgress(payload) {
    if (!this.#eventsExpected()) return;
    this.progress = payload;
    const rounded = Math.round(payload.percent);
    if (rounded > this.displayPercent) this.displayPercent = rounded;
    if (payload.message !== this.displayMessage) this.displayMessage = payload.message;
  }

  /** @param {any} payload */
  handleComplete(payload) {
    if (!this.#eventsExpected()) return;
    this.outputPath = payload.path;
    this.#stopTimer();
    this.completionData = {
      filename: payload.path.split(/[\\/]/).pop() ?? "output.m4b",
      elapsed: this.elapsedSeconds,
      fileCount: this.#startStats?.fileCount ?? fileStore.count,
      totalDuration: this.#startStats?.totalDuration ?? fileStore.totalDuration,
      sizeBytes: payload.size_bytes,
      containerLabel: outputContainerLabel(),
      codecLabel: outputCodecLabel(),
    };
    appStore.screen = "complete";
    appStore.announce("Audio file created successfully");
  }

  /** @param {any} payload */
  handleError(payload) {
    if (!this.#eventsExpected()) return;
    this.#stopTimer();
    appStore.error = String(payload);
    appStore.screen = "setup";
    appStore.announce("Conversion failed: " + String(payload));
  }

  handleCancelled() {
    if (!this.#eventsExpected()) return;
    this.#stopTimer();
    appStore.warning = "Conversion cancelled — your chapters and settings are unchanged.";
    appStore.screen = "setup";
    appStore.announce("Conversion cancelled");
  }

  async start() {
    if (fileStore.count < 1) return;
    if (fileStore.probing) {
      appStore.error = "Still reading files — wait for probing to finish, then try again.";
      return;
    }

    const folder = settingsStore.outputDir.trim();
    const filename = sanitizeFilename(settingsStore.outputFilename);
    const missing = [];
    if (!folder) missing.push("output folder");
    if (!filename) missing.push("filename");
    if (missing.length > 0) {
      appStore.validationAttempted = true;
      appStore.error = `Please fill the ${missing.join(" and ")} before binding.`;
      return;
    }
    // Reflect the cleaned values in the UI so what the user sees is what
    // gets written.
    settingsStore.outputDir = folder;
    settingsStore.outputFilename = filename;

    appStore.validationAttempted = false;
    appStore.error = null;
    this.outputPath = null;

    try {
      const preflight = await preflightCheck({
        files: fileStore.items.map(f => f.path),
        outputDir: folder,
        outputFilename: filename,
        outputExtension: outputExtension(),
      });
      if (!preflight.ok) { appStore.error = preflight.errors.join("\n"); return; }
      if (preflight.warnings.length > 0) appStore.warning = preflight.warnings.join("\n");
    } catch (e) {
      appStore.error = String(e);
      return;
    }

    this.progress = { stage: STAGES.PREPARING, percent: 0, message: "Starting" };
    this.displayPercent = 0;
    this.displayMessage = "Starting";
    this.#startStats = { fileCount: fileStore.count, totalDuration: fileStore.totalDuration };
    appStore.screen = "converting";
    this.#startTimer();
    appStore.announce("Conversion started");

    const { output_codec, force_transcode, wrap_in_mp4 } = backendTuple(
      fileStore.items,
      settingsStore.mp3FormatChoice,
      settingsStore.qualityMode,
    );

    try {
      await mergeAudioFiles({
        files: fileStore.items.map(f => ({ path: f.path, chapter_name: f.chapter_name })),
        output_dir: folder,
        output_filename: filename,
        title: metadataStore.title || null,
        artist: metadataStore.artist || null,
        album: metadataStore.album || null,
        narrator: metadataStore.narrator || null,
        year: metadataStore.year || null,
        cover_art_path: fileStore.coverArtPath,
        bitrate: settingsStore.bitrate * (settingsStore.mono ? 1 : 2),
        mono: settingsStore.mono,
        force_transcode,
        output_codec,
        wrap_in_mp4,
        durations: fileStore.items.map(f => f.duration),
      });
    } catch (e) {
      this.#stopTimer();
      appStore.error = String(e);
      appStore.screen = "setup";
    }
  }

  async cancel() {
    try { await cancelMerge(); } catch { /* ignore */ }
  }

  async revealOutput() {
    if (this.outputPath) {
      try { await revealInFolder(this.outputPath); } catch { /* ignore */ }
    }
  }

  convertAnother() {
    appStore.clearAll();
    this.outputPath = null;
    this.completionData = null;
    this.progress = { stage: "", percent: 0, message: "" };
    this.elapsedSeconds = 0;
  }

  destroy() { this.#stopTimer(); }
}

export const conversionStore = new ConversionStore();
