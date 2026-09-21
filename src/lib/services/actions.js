import { appStore } from "$lib/stores/app.svelte.js";
import { fileStore } from "$lib/stores/files.svelte.js";
import { settingsStore } from "$lib/stores/settings.svelte.js";
import { metadataStore } from "$lib/stores/metadata.svelte.js";
import { browseFiles, browseFolderAndResolve, browseImage, confirmAsk, setCustomCoverArt } from "$lib/services/tauri.js";

/**
 * Shared destructive-clear flow: every path that wipes the session (Cancel
 * button, Clear button, keyboard shortcut) confirms through the same dialog.
 * Returns true if the session was cleared.
 */
export async function clearAllWithConfirm() {
  if (fileStore.count > 0) {
    const ok = await confirmAsk(
      `Discard ${fileStore.count} chapter${fileStore.count !== 1 ? "s" : ""} and metadata?`,
      { title: "Discard changes", okLabel: "Discard", cancelLabel: "Keep" },
    );
    if (!ok) return false;
  }
  appStore.clearAll();
  return true;
}

/**
 * Shared add-files flow: probe, populate metadata/settings from first file.
 * @param {string[]} paths
 * @param {string | null} [folderName]
 */
export async function addFiles(paths, folderName = null) {
  const wasEmpty = fileStore.count === 0;
  const result = await fileStore.add(paths, {
    folderName,
    setError: (msg) => { appStore.error = msg; },
    setWarning: (msg) => { appStore.warning = msg; },
    announce: (msg) => appStore.announce(msg),
  });
  if (result?.firstFile) {
    if (wasEmpty) await settingsStore.setOutputDirForNewBatch(result.firstFile.path);
    metadataStore.populateFrom(result.firstFile);
    settingsStore.setFilenameFrom(folderName, result.firstFile);
  }
}

export async function addFilesFromBrowse() {
  const paths = await browseFiles();
  if (paths) await addFiles(paths);
}

export async function addFilesFromFolder() {
  const result = await browseFolderAndResolve();
  if (result) await addFiles(result.paths, result.folderName);
}

/**
 * Shared set-cover flow, used by the cover square's click and by images
 * dropped onto it: validate through the backend, then swap the cover in.
 * @param {string} path
 */
export async function setCoverFromPath(path) {
  if (fileStore.coverPending) return;
  fileStore.coverPending = true;
  try {
    const art = await setCustomCoverArt(path);
    fileStore.setCover(art.data_uri, art.file_path);
    appStore.announce("Cover art set");
  } catch (e) {
    appStore.error = String(e);
  } finally {
    fileStore.coverPending = false;
  }
}

export async function chooseCoverArt() {
  if (fileStore.coverPending) return;
  const path = await browseImage();
  if (path) await setCoverFromPath(path);
}
