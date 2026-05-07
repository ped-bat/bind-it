const STORAGE_KEY = "bindit:quality";
const OUTPUT_DIR_KEY = "bindit:outputDir";
const OUTPUT_FORMAT_KEY = "bindit:outputFormat";

export const FORBIDDEN_FILENAME_CHARS = /[/\\:*?"<>|]/g;

/** @typedef {"original" | "original-m4b" | "aac" | "alac"} OutputFormat */

const VALID_FORMATS = /** @type {OutputFormat[]} */ (["original", "original-m4b", "aac", "alac"]);

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

function loadOutputFormat() {
  try {
    const raw = localStorage.getItem(OUTPUT_FORMAT_KEY);
    if (raw && VALID_FORMATS.includes(/** @type {OutputFormat} */ (raw))) {
      return /** @type {OutputFormat} */ (raw);
    }
  } catch { /* ignore */ }
  return /** @type {OutputFormat} */ ("original");
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

function saveOutputFormat(/** @type {OutputFormat} */ fmt) {
  try { localStorage.setItem(OUTPUT_FORMAT_KEY, fmt); } catch { /* ignore */ }
}

function saveOutputDir(/** @type {string} */ dir) {
  try {
    if (dir) localStorage.setItem(OUTPUT_DIR_KEY, dir);
    else localStorage.removeItem(OUTPUT_DIR_KEY);
  } catch { /* ignore */ }
}

class SettingsStore {
  outputDir = $state(loadOutputDir());
  outputFilename = $state("audio");

  #saved = loadQuality();
  bitrate = $state(this.#saved?.bitrate ?? 64);
  mono = $state(this.#saved?.mono ?? true);
  /** @type {"lossless" | "compress"} */
  qualityMode = $state(this.#saved?.qualityMode ?? (this.#saved?.lossless === false ? "compress" : "lossless"));
  /** @type {OutputFormat} */
  outputFormat = $state(loadOutputFormat());

  /**
   * Set output dir from first file path. Always overwrites — drag/drop is an
   * explicit signal of intent, and stale values from localStorage shouldn't win.
   * @param {string} filePath
   */
  setOutputDirFromFile(filePath) {
    const parts = filePath.split(/[\\/]/);
    parts.pop();
    this.outputDir = parts.join("/");
  }

  /**
   * Auto-set filename from folder name or metadata
   * @param {string | null} folderName
   * @param {any} file - first probed file
   */
  setFilenameFrom(folderName, file) {
    if (this.outputFilename && this.outputFilename !== "audio") return;
    const name = folderName || file?.album || file?.title || "audio";
    this.outputFilename = name.replace(FORBIDDEN_FILENAME_CHARS, "");
  }

  persistQuality() {
    saveQuality(this.bitrate, this.mono, this.qualityMode);
  }

  persistOutputDir() {
    saveOutputDir(this.outputDir);
  }

  persistOutputFormat() {
    saveOutputFormat(this.outputFormat);
  }

  /**
   * Apply a format selection. AAC and ALAC pin the quality mode (AAC is
   * always lossy, ALAC is always lossless), so we set qualityMode to match
   * to keep the UI in sync with what the binding will actually do.
   * @param {OutputFormat} fmt
   */
  setOutputFormat(fmt) {
    this.outputFormat = fmt;
    if (fmt === "aac") this.qualityMode = "compress";
    else if (fmt === "alac") this.qualityMode = "lossless";
    this.persistOutputFormat();
    this.persistQuality();
  }

  reset() {
    this.outputDir = "";
    this.outputFilename = "audio";
  }
}

export const settingsStore = new SettingsStore();
