import { fileStore } from "$lib/stores/files.svelte.js";
import { settingsStore } from "$lib/stores/settings.svelte.js";

/**
 * Derive the output container extension from the chosen output format and
 * the current input set. The only case that yields `.mp3` is "Original" with
 * an all-MP3 source — every other combination produces `.m4b`.
 */
export function outputExtension() {
  const fmt = settingsStore.outputFormat;
  if (fmt !== "original") return "m4b";
  const items = fileStore.items;
  if (items.length === 0) return "m4b";
  const allMp3 = items.every(f => f.codec === "mp3");
  return allMp3 ? "mp3" : "m4b";
}
