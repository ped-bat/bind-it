import { readdir, readFile, writeFile, stat } from 'node:fs/promises';
import { join, extname } from 'node:path';
import { defineConfig } from 'vite';

/**
 * Compresses every PNG in the build output in place and writes a WebP sibling
 * next to it. Runs after Vite has copied public/ into dist, so the sources
 * under public/ are never touched - what ships is optimised, what is
 * committed stays pristine. index.html references both via <picture>, so
 * browsers take the WebP and fall back to the PNG.
 *
 * sharp is a devDependency; the whole plugin is skipped if it is missing.
 */
function optimizeImages({ webp = [] } = {}) {
  let outDir = 'dist';
  return {
    name: 'optimize-images',
    apply: 'build',
    configResolved(config) {
      outDir = config.build.outDir;
    },
    async closeBundle() {
      let sharp;
      try {
        sharp = (await import('sharp')).default;
      } catch {
        this.warn('sharp not installed - skipping image optimisation');
        return;
      }

      const files = (await readdir(outDir)).filter((f) => extname(f) === '.png');
      for (const file of files) {
        const path = join(outDir, file);
        const before = (await stat(path)).size;
        const source = await readFile(path);

        // Lossless-ish PNG: palette quantisation is visually identical here
        // (flat UI colours) and roughly halves the size.
        const png = await sharp(source)
          .png({ compressionLevel: 9, palette: true, quality: 90, effort: 10 })
          .toBuffer();
        if (png.length < before) await writeFile(path, png);

        const pct = (n) => `${Math.round((1 - n / before) * 100)}%`;
        let note = `${file}: ${before} -> png ${png.length} (-${pct(png.length)})`;

        // Only where index.html has a <picture> ready for it. WebP keeps the
        // alpha channel, which the screenshot's transparent gaps rely on.
        if (webp.includes(file)) {
          const out = await sharp(source).webp({ quality: 85, effort: 6 }).toBuffer();
          await writeFile(path.replace(/\.png$/, '.webp'), out);
          note += `, webp ${out.length} (-${pct(out.length)})`;
        }
        this.info(note);
      }
    },
  };
}

export default defineConfig({
  base: './',
  plugins: [optimizeImages({ webp: ['app-screenshot.png'] })],
  server: {
    port: 1421,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    assetsInlineLimit: 4096,
  },
});
