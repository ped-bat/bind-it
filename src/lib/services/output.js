import { fileStore } from "$lib/stores/files.svelte.js";
import { settingsStore } from "$lib/stores/settings.svelte.js";

const ALWAYS_TRANSCODE_CODECS = ["flac", "wav", "wma"];

/**
 * Inputs the labelling helpers operate on. Either pulled live from the
 * stores (default) or supplied explicitly so they can be computed at the
 * moment of conversion start, before the user interacts further.
 *
 * @typedef {{
 *   items: { codec: string }[],
 *   mp3Choice: "mp3" | "mp3-m4b" | "reencode",
 *   qualityMode: "lossless" | "compress",
 * }} LabelInputs
 */

/** @returns {LabelInputs} */
function liveInputs() {
  return {
    items: fileStore.items,
    mp3Choice: settingsStore.mp3FormatChoice,
    qualityMode: settingsStore.qualityMode,
  };
}

/**
 * Derive the output container extension. The only case that yields `.mp3`
 * is an all-MP3 input set with the user's `MP3` choice.
 * @param {LabelInputs} [inputs]
 */
export function outputExtension(inputs = liveInputs()) {
  const { items, mp3Choice } = inputs;
  if (items.length === 0) return "m4b";
  const allMp3 = items.every(f => f.codec === "mp3");
  if (allMp3 && mp3Choice === "mp3") return "mp3";
  return "m4b";
}

/**
 * Container label for the "Expected output" / completion summary.
 * @param {LabelInputs} [inputs]
 */
export function outputContainerLabel(inputs = liveInputs()) {
  return outputExtension(inputs).toUpperCase();
}

/**
 * Codec label the merged file will carry. For preserve paths we report the
 * source codec; for transcode paths we report the target.
 * @param {LabelInputs} [inputs]
 */
export function outputCodecLabel(inputs = liveInputs()) {
  const { items, mp3Choice, qualityMode } = inputs;
  if (items.length === 0) return "AAC";
  const allMp3 = items.every(f => f.codec === "mp3");
  const codecSet = new Set(items.map(f => f.codec));
  const hasAlwaysTranscode = items.some(f => ALWAYS_TRANSCODE_CODECS.includes(f.codec));
  const isMixed = codecSet.size > 1;
  const lossless = qualityMode === "lossless";

  if (allMp3) {
    if (mp3Choice === "mp3" || mp3Choice === "mp3-m4b") return "MP3";
    return lossless ? "ALAC" : "AAC";
  }
  if (!lossless) return "AAC";
  if (hasAlwaysTranscode || isMixed) return "ALAC";
  // Preserve path on a single uniform codec.
  const only = [...codecSet][0];
  if (only === "aac") return "AAC";
  if (only === "alac") return "ALAC";
  return "AAC";
}
