const STORAGE_KEY = "bindit:quality";
const OUTPUT_DIR_KEY = "bindit:outputDir";
const MP3_CHOICE_KEY = "bindit:mp3FormatChoice";

export const FORBIDDEN_FILENAME_CHARS = /[/\\:*?"<>|]/g;

/** @typedef {"mp3" | "mp3-m4b" | "reencode"} Mp3FormatChoice */

const VALID_MP3_CHOICES = /** @type {Mp3FormatChoice[]} */ (["mp3", "mp3-m4b", "reencode"]);

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

function loadMp3Choice() {
  try {
    const raw = localStorage.getItem(MP3_CHOICE_KEY);
    if (raw && VALID_MP3_CHOICES.includes(/** @type {Mp3FormatChoice} */ (raw))) {
      return /** @type {Mp3FormatChoice} */ (raw);
    }
  } catch { /* ignore */ }
  return /** @type {Mp3FormatChoice} */ ("mp3");
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

function saveMp3Choice(/** @type {Mp3FormatChoice} */ c) {
  try { localStorage.setItem(MP3_CHOICE_KEY, c); } catch { /* ignore */ }
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
  /**
   * Selection within the MP3-only dropdown. Only meaningful when the input
   * set is 100% MP3. Persists across sessions so the user's preference survives
   * regardless of the file set currently loaded.
   * @type {Mp3FormatChoice}
   */
  mp3FormatChoice = $state(loadMp3Choice());

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

  /** @param {Mp3FormatChoice} c */
  setMp3FormatChoice(c) {
    this.mp3FormatChoice = c;
    saveMp3Choice(c);
  }

  reset() {
    this.outputDir = "";
    this.outputFilename = "audio";
  }
}

export const settingsStore = new SettingsStore();
