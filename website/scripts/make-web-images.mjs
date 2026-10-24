// Renders the compressed copies of the hero screenshot and the header logo.
// Not part of the build: run `npm run web:images` after changing
// public/assets/hero-screenshot.png or public/logo.svg, and commit the output.
// The PNG and SVG stay as the sources (and for structured data and the OG card).
import path from 'node:path';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const pub = (file) => path.join(root, 'public', file);

const outputs = [];

for (const width of [720, 1440]) {
  const base = sharp(pub('assets/hero-screenshot.png')).resize({ width });
  outputs.push(
    base.clone().webp({ quality: 82, effort: 6 }).toFile(pub(`assets/hero-screenshot-${width}.webp`)),
    base.clone().avif({ quality: 60, effort: 6 }).toFile(pub(`assets/hero-screenshot-${width}.avif`)),
  );
}

// Header mark: shown at 36 px tall, so 2x and 3x.
for (const height of [72, 108]) {
  outputs.push(
    sharp(pub('logo.svg'), { density: 300 })
      .resize({ height })
      .webp({ quality: 90, alphaQuality: 100, effort: 6 })
      .toFile(pub(`logo-${height}.webp`)),
  );
}

for (const info of await Promise.all(outputs)) {
  console.log(`${info.width}x${info.height} ${info.format} ${Math.round(info.size / 1024)} KB`);
}
