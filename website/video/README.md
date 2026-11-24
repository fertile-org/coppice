# Marketing hero (Remotion)

Standalone Remotion package for the Coppice homepage / README hero. Not part of the Astro site build or website CI.

```bash
# From the repo root — renders mp4/webm/gif/poster into website/public/assets/
# and static/marketing.gif, then runs website web:images.
make hero-video
```

- Composition: `Hero` (~8 s, 1440×900, loops, no zoom)
- Remotion **4.0.543** (pinned)
- Kit UI under `src/kit/` (seeded from `coppice-content`, retargeted for the marketing canvas)
- Preview: `cd website/video && npm run studio`

Docs stills stay on Playwright (`make screenshot`) and must not overwrite these binaries.
