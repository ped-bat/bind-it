import { appStore } from "./app.svelte.js";
import { fileStore } from "./files.svelte.js";
import { metadataStore } from "./metadata.svelte.js";
import { settingsStore } from "./settings.svelte.js";
import { preflightCheck, mergeAudiobook, cancelMerge, revealInFolder } from "$lib/services/tauri.js";

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
  /** @type {{ filename: string, elapsed: number, fileCount: number, totalDuration: number, sizeBytes: number } | null} */
  completionData = $state(null);

  /** @type {ReturnType<typeof setInterval> | null} */
  #timer = null;

  #startTimer() {
    this.elapsedSeconds = 0;
    this.#timer = setInterval(() => { this.elapsedSeconds += 1; }, 1000);
  }

  #stopTimer() {
    if (this.#timer) { clearInterval(this.#timer); this.#timer = null; }
  }

  /** @param {any} payload */
  handleProgress(payload) {
    this.progress = payload;
    const rounded = Math.round(payload.percent);
    if (rounded > this.displayPercent) this.displayPercent = rounded;
    if (payload.message !== this.displayMessage) this.displayMessage = payload.message;
  }

  /** @param {any} payload */
  handleComplete(payload) {
    this.outputPath = payload.path;
    this.#stopTimer();
    this.completionData = {
      filename: payload.path.split(/[\\/]/).pop() ?? "output.m4b",
      elapsed: this.elapsedSeconds,
      fileCount: fileStore.count,
      totalDuration: fileStore.totalDuration,
      sizeBytes: payload.size_bytes,
    };
    appStore.screen = "complete";
    appStore.announce("Audiobook created successfully");
  }

  /** @param {any} payload */
  handleError(payload) {
    this.#stopTimer();
    appStore.error = String(payload);
    appStore.screen = "setup";
    appStore.announce("Conversion failed: " + String(payload));
  }

  handleCancelled() {
    this.#stopTimer();
    appStore.error = "Conversion cancelled";
    appStore.screen = "setup";
  }

  async start() {
    if (fileStore.count < 1 || !settingsStore.outputDir || !settingsStore.outputFilename) return;
    appStore.error = null;
    this.outputPath = null;

    try {
      const preflight = await preflightCheck({
        files: fileStore.items.map(f => f.path),
        outputDir: settingsStore.outputDir,
        outputFilename: settingsStore.outputFilename,
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
    appStore.screen = "converting";
    this.#startTimer();
    appStore.announce("Conversion started");

    const isLossless = settingsStore.qualityMode === "lossless";
    const allAac = fileStore.items.length > 0 && fileStore.items.every(f => f.codec === "aac");
    // Lossless + all AAC → remux via AAC pipeline (force_transcode=false lets it pick remux).
    // Lossless + mixed → ALAC encode.
    // Compress → AAC encode (force).
    const output_codec = isLossless && !allAac ? "alac" : "aac";
    const force_transcode = !isLossless;

    try {
      await mergeAudiobook({
        files: fileStore.items.map(f => ({ path: f.path, chapter_name: f.chapter_name })),
        output_dir: settingsStore.outputDir,
        output_filename: settingsStore.outputFilename,
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
