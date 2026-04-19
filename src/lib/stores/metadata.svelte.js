class MetadataStore {
  title = $state("");
  artist = $state("");
  album = $state("");
  narrator = $state("");
  year = $state("");

  /** @param {any} file */
  populateFrom(file) {
    if (!file || this.title) return;
    this.title = file.album || file.title || "";
    this.artist = file.artist || "";
    this.album = file.album || "";
    this.narrator = file.narrator || "";
    this.year = file.year || "";
  }

  reset() {
    this.title = "";
    this.artist = "";
    this.album = "";
    this.narrator = "";
    this.year = "";
  }
}

export const metadataStore = new MetadataStore();
