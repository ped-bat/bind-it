import { probeFiles, getCoverArt, getMergePlan } from "$lib/services/tauri.js";

class FileStore {
  /** @type {any[]} */
  items = $state([]);
  probing = $state(false);
  // Concurrent add() calls each own a slot; the spinner only stops when the
  // last one finishes.
  #probesInFlight = 0;
  // Monotonic token so an older #refreshMergePlan can't overwrite a newer
  // one when responses resolve out of order.
  #planToken = 0;
  /** @type {string | null} */
  coverArt = $state(null);
  /** @type {string | null} */
  coverArtPath = $state(null);
  /** @type {{ strategy: string, needs_transcode: string[], total_duration: number } | null} */
  mergePlan = $state(null);

  get totalDuration() {
    return this.items.reduce((sum, f) => sum + f.duration, 0);
  }

  get count() {
    return this.items.length;
  }

  get avgBitrate() {
    if (this.items.length === 0) return 0;
    return this.items.reduce((sum, f) => sum + (f.bitrate || 0), 0) / this.items.length;
  }

  /**
   * Add files by path, probe them, fetch cover art and merge plan.
   * Returns { firstFile, folderName } on success, null on error.
   * @param {string[]} paths
   * @param {{ folderName?: string | null, setError: (msg: string) => void, setWarning: (msg: string) => void, announce: (msg: string) => void }} opts
   */
  async add(paths, opts) {
    const { folderName = null, setError, setWarning, announce } = opts;
    this.#probesInFlight++;
    this.probing = true;
    try {
      const result = await probeFiles(paths);
      if (result.warnings?.length > 0) {
        setWarning(result.warnings.join("\n"));
      }

      const existingPaths = new Set(this.items.map(f => f.path));
      const newFiles = result.files.filter((/** @type {any} */ f) => !existingPaths.has(f.path));
      this.items = [...this.items, ...newFiles];

      if (newFiles.length > 0) {
        announce(`${newFiles.length} file${newFiles.length !== 1 ? "s" : ""} added`);
      }

      // Cover art is a nice-to-have: a failure here must not abort the add
      // or skip the merge-plan refresh.
      if (!this.coverArt) {
        try {
          const art = await getCoverArt(this.items.map(f => f.path));
          if (art) {
            this.coverArt = art.data_uri;
            this.coverArtPath = art.file_path;
          }
        } catch (e) {
          console.warn("getCoverArt failed:", e);
        }
      }

      await this.#refreshMergePlan();
      return { firstFile: this.items[0] ?? null, folderName };
    } catch (e) {
      setError(String(e));
      return null;
    } finally {
      this.#probesInFlight--;
      if (this.#probesInFlight === 0) this.probing = false;
    }
  }

  /** @param {number} index */
  remove(index) {
    this.items = this.items.filter((_, i) => i !== index);
    this.#refreshMergePlan();
  }

  /** @param {number} index @param {string} name */
  updateChapterName(index, name) {
    this.items = this.items.map((f, i) => i === index ? { ...f, chapter_name: name } : f);
  }

  /** @param {number} from @param {number} to */
  reorder(from, to) {
    const arr = [...this.items];
    const [moved] = arr.splice(from, 1);
    arr.splice(to, 0, moved);
    this.items = arr;
  }

  async #refreshMergePlan() {
    const token = ++this.#planToken;
    if (this.items.length < 1) { this.mergePlan = null; return; }
    try {
      const plan = await getMergePlan(this.items.map(f => ({
        path: f.path, codec: f.codec, sample_rate: f.sample_rate,
        channels: f.channels, duration: f.duration,
      })));
      if (token === this.#planToken) this.mergePlan = plan;
    } catch (e) {
      // Keep the previous plan rather than blanking the Quality panel with
      // no explanation; the merge itself recomputes everything backend-side.
      console.warn("getMergePlan failed:", e);
    }
  }

  clear() {
    this.items = [];
    this.coverArt = null;
    this.coverArtPath = null;
    this.mergePlan = null;
  }
}

export const fileStore = new FileStore();
