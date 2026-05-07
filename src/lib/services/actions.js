import { appStore } from "$lib/stores/app.svelte.js";
import { fileStore } from "$lib/stores/files.svelte.js";
import { settingsStore } from "$lib/stores/settings.svelte.js";
import { metadataStore } from "$lib/stores/metadata.svelte.js";
import { browseFiles, browseFolderAndResolve } from "$lib/services/tauri.js";

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
    if (wasEmpty) settingsStore.setOutputDirFromFile(result.firstFile.path);
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
