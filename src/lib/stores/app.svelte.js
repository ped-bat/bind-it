import { fileStore } from "./files.svelte.js";
import { metadataStore } from "./metadata.svelte.js";
import { settingsStore } from "./settings.svelte.js";

class AppStore {
  /** @type {"setup" | "converting" | "complete"} */
  screen = $state("setup");
  /** @type {string | null} */
  #error = $state(null);
  dismissingError = $state(false);
  /** @type {ReturnType<typeof setTimeout> | null} */
  #dismissTimer = null;
  /** @type {string | null} */
  warning = $state(null);
  /** @type {boolean | null} */
  ffmpegOk = $state(null);
  dragOver = $state(false);
  liveAnnouncement = $state("");
  // Set to true once the user has attempted to submit; flips required-field
  // validation styling on. Cleared on successful submit / clearAll.
  validationAttempted = $state(false);

  get error() {
    return this.#error;
  }

  /**
   * Assigning a new error cancels any in-flight dismiss animation timer —
   * otherwise a dismiss started up to 400 ms earlier wipes the new message.
   * @param {string | null} msg
   */
  set error(msg) {
    if (this.#dismissTimer) {
      clearTimeout(this.#dismissTimer);
      this.#dismissTimer = null;
    }
    this.dismissingError = false;
    this.#error = msg;
  }

  /** @param {string} msg */
  announce(msg) {
    this.liveAnnouncement = "";
    requestAnimationFrame(() => { this.liveAnnouncement = msg; });
  }

  dismissError() {
    if (this.dismissingError || this.#error === null) return;
    this.dismissingError = true;
    this.#dismissTimer = setTimeout(() => {
      this.#error = null;
      this.dismissingError = false;
      this.#dismissTimer = null;
    }, 400);
  }

  dismissWarning() {
    this.warning = null;
  }

  clearAll() {
    fileStore.clear();
    metadataStore.reset();
    settingsStore.reset();
    this.error = null;
    this.warning = null;
    this.screen = "setup";
    this.validationAttempted = false;
  }
}

export const appStore = new AppStore();
