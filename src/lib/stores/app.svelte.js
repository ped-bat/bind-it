import { fileStore } from "./files.svelte.js";
import { metadataStore } from "./metadata.svelte.js";
import { settingsStore } from "./settings.svelte.js";

class AppStore {
  /** @type {"setup" | "converting" | "complete"} */
  screen = $state("setup");
  /** @type {string | null} */
  error = $state(null);
  dismissingError = $state(false);
  /** @type {string | null} */
  warning = $state(null);
  /** @type {boolean | null} */
  ffmpegOk = $state(null);
  dragOver = $state(false);
  liveAnnouncement = $state("");
  // Set to true once the user has attempted to submit; flips required-field
  // validation styling on. Cleared on successful submit / clearAll.
  validationAttempted = $state(false);

  /** @param {string} msg */
  announce(msg) {
    this.liveAnnouncement = "";
    requestAnimationFrame(() => { this.liveAnnouncement = msg; });
  }

  dismissError() {
    if (this.dismissingError) return;
    this.dismissingError = true;
    setTimeout(() => {
      this.error = null;
      this.dismissingError = false;
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
    this.dismissingError = false;
    this.validationAttempted = false;
  }
}

export const appStore = new AppStore();
