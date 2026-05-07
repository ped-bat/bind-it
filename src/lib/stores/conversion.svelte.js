import { appStore } from "./app.svelte.js";
import { fileStore } from "./files.svelte.js";
import { metadataStore } from "./metadata.svelte.js";
import { settingsStore } from "./settings.svelte.js";
import { preflightCheck, mergeAudioFiles, cancelMerge, revealInFolder, confirmAsk } from "$lib/services/tauri.js";
import { outputExtension } from "$lib/services/output.js";

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
    appStore.announce("Audio file created successfully");
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

    const fmt = settingsStore.outputFormat;
    const hasMp3 = fileStore.items.some(f => f.codec === "mp3");

    // The two "Original" formats keep the source codec, which carries known
    // compatibility quirks with Apple players for MP3 sources. Make the user
    // confirm before we kick off a long conversion they may have to redo.
    if (fmt === "original" && hasMp3) {
      const ok = await confirmAsk(
        "Chapters added as ID3v2 chapters. This combination can have some issues in Apple Books app.",
        { title: "Confirm output format", okLabel: "Bind anyway", cancelLabel: "Cancel" },
      );
      if (!ok) return;
    } else if (fmt === "original-m4b" && hasMp3) {
      const ok = await confirmAsk(
        "This combination can have some issues in Apple Books app.",
        { title: "Confirm output format", okLabel: "Bind anyway", cancelLabel: "Cancel" },
      );
      if (!ok) return;
    } else if (fmt === "original" || fmt === "original-m4b") {
      const ok = await confirmAsk(
        `Output will preserve the source codec inside ${fmt === "original-m4b" ? "an M4B" : "the matching"} container. Continue?`,
        { title: "Confirm output format", okLabel: "Bind", cancelLabel: "Cancel" },
      );
      if (!ok) return;
    }

    try {
      const preflight = await preflightCheck({
        files: fileStore.items.map(f => f.path),
        outputDir: settingsStore.outputDir,
        outputFilename: settingsStore.outputFilename,
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
    appStore.screen = "converting";
    this.#startTimer();
    appStore.announce("Conversion started");

    // Map the user's format choice onto the backend's (output_codec,
    // force_transcode, wrap_in_mp4) tuple.
    //   original      → preserve source codec (remux when possible);
    //                   wrap_in_mp4=false so MP3 sources land in .mp3
    //   original-m4b  → preserve source codec but force MP4 container
    //   aac           → re-encode to AAC, .m4b
    //   alac          → encode to ALAC, .m4b
    const NATIVE_M4B_CODECS = ["aac", "mp3", "alac"];
    const firstCodec = fileStore.items[0]?.codec;
    const allSameNative = fileStore.items.length > 0
      && NATIVE_M4B_CODECS.includes(firstCodec)
      && fileStore.items.every(f => f.codec === firstCodec);

    let output_codec;
    let force_transcode;
    let wrap_in_mp4;
    if (fmt === "aac") {
      output_codec = "aac";
      force_transcode = true;
      wrap_in_mp4 = false;
    } else if (fmt === "alac") {
      output_codec = "alac";
      force_transcode = false;
      wrap_in_mp4 = false;
    } else {
      // "original" and "original-m4b" share the codec selection logic.
      output_codec = allSameNative ? "aac" : "alac";
      force_transcode = false;
      wrap_in_mp4 = fmt === "original-m4b";
    }

    try {
      await mergeAudioFiles({
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
