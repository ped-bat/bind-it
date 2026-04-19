const STORAGE_KEY = "bindery:quality";
const OUTPUT_DIR_KEY = "bindery:outputDir";

export const FORBIDDEN_FILENAME_CHARS = /[/\\:*?"<>|]/g;

function loadQuality() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return null;
}

function loadOutputDir() {
  try {
    return localStorage.getItem(OUTPUT_DIR_KEY) || "";
  } catch { return ""; }
}

/**
 * @param {number} bitrate
 * @param {boolean} mono
 * @param {"lossless" | "compress"} qualityMode
 */
function saveQuality(bitrate, mono, qualityMode) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ bitrate, mono, qualityMode }));
  } catch { /* ignore */ }
}

function saveOutputDir(/** @type {string} */ dir) {
  try {
    if (dir) localStorage.setItem(OUTPUT_DIR_KEY, dir);
  } catch { /* ignore */ }
}

class SettingsStore {
  outputDir = $state(loadOutputDir());
  outputFilename = $state("audiobook");

  #saved = loadQuality();
  bitrate = $state(this.#saved?.bitrate ?? 64);
  mono = $state(this.#saved?.mono ?? true);
  /** @type {"lossless" | "compress"} */
  qualityMode = $state(this.#saved?.qualityMode ?? (this.#saved?.lossless === false ? "compress" : "lossless"));

  /**
   * Auto-set output dir from first file path
   * @param {string} filePath
   */
  setOutputDirFromFile(filePath) {
    if (this.outputDir) return;
    const parts = filePath.split("/");
    parts.pop();
    this.outputDir = parts.join("/");
  }

  /**
   * Auto-set filename from folder name or metadata
   * @param {string | null} folderName
   * @param {any} file - first probed file
   */
  setFilenameFrom(folderName, file) {
    if (this.outputFilename && this.outputFilename !== "audiobook") return;
    const name = folderName || file?.album || file?.title || "audiobook";
    this.outputFilename = name.replace(FORBIDDEN_FILENAME_CHARS, "");
  }

  persistQuality() {
    saveQuality(this.bitrate, this.mono, this.qualityMode);
  }

  persistOutputDir() {
    saveOutputDir(this.outputDir);
  }

  reset() {
    this.outputDir = "";
    this.outputFilename = "audiobook";
  }
}

export const settingsStore = new SettingsStore();
