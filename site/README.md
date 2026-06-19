# Min-Mozhi website (`site/`)

The marketing + docs site for **Min-Mozhi** (மின்மொழி) — landing page, the guide /
language spec, and an in-browser playground that runs the real compiler compiled
to WebAssembly. Public home: `mimz.naveenr.in` (not live until the maintainer
flips it — see _Deploy_).

## Stack

- **Astro 6** (static output) with **React 19** islands for the interactive bits.
- **Tailwind v4** (via `@tailwindcss/vite`) + a small set of design tokens and
  utility classes in `src/styles/global.css`. **No inline styles** anywhere
  (`style=` is disallowed — keep everything in classes).
- **Shiki** for code blocks (the `mimz` grammar is loaded from the VS Code
  extension's TextMate file), **Pagefind** for search, `@astrojs/sitemap`.
- Fonts are **self-hosted** via Fontsource (Inter, JetBrains Mono, Noto Sans
  Tamil) — no Google Fonts request, so the CSP needs no font CDN.
- `@astrojs/vercel` adapter; security headers + CSP live in `vercel.json`.

## Layout

```text
public/            favicon.svg (waveform mark) · favicon-32 · apple-touch-icon
                   mascot.png (peacock) · mascot-emblem.png · og.png · robots.txt
src/
  components/
    Logo.astro          the waveform logo mark (matches the favicon)
    Mascot.astro        the peacock mascot (full / emblem variants)
    Wordmark.astro      Logo + மின்மொழி / Min-Mozhi
    Nav / Footer / Sidebar
    Hero.tsx            interactive 2D oscilloscope (play/pause · speed · signal)
    features/           reveal-on-scroll landing illustrations
      SafeByDefault.tsx · BuiltToTeach.tsx · Trilingual.tsx · useReveal.ts
    Playground.tsx      the in-browser console island
    WaveformViewer.tsx  canvas VCD waveform renderer
  layouts/         Base.astro · Docs.astro
  pages/           index.astro · playground.astro · 404.astro · guide/ · spec/
  lib/wasm/        generated wasm-bindgen glue (git-ignored — see below)
  styles/global.css
```

## Develop

```sh
npm install
npm run dev        # http://localhost:4321
npm run check      # astro check (type + a11y diagnostics)
npm run build      # static build → dist/ (also runs Pagefind)
```

### Playground / WASM

The playground imports generated wasm-bindgen glue from `src/lib/wasm/`, which is
**git-ignored and must be built from `crates/mimz-wasm`** before the playground
works. See [`docs/BUILD.md`](../docs/BUILD.md) and the deploy workflow
(`.github/workflows/deploy-site.yml`) for the exact, version-pinned commands
(`wasm-bindgen` 0.2.125). Without it the rest of the site still builds; the
playground island is the only thing that needs it.

## Deploy

CI-prebuilt to Vercel (approach B): `.github/workflows/deploy-site.yml` builds the
WASM + Astro and runs `vercel deploy --prebuilt`. PRs and `master` publish a
**preview**; `workflow_dispatch` with `target=production` publishes **prod**.
Requires the `VERCEL_TOKEN` / `VERCEL_ORG_ID` / `VERCEL_PROJECT_ID` repo secrets
and a one-time `vercel link` (Root Directory = `site`). Going public also needs
the `mimz.naveenr.in` DNS CNAME and the maintainer's flip (R12).

## Conventions

- Keep `style=` out of `.astro`/`.tsx`; add a class to `global.css` instead.
- Honour `prefers-reduced-motion` in every animation (the hero and all three
  feature illustrations fall back to a static frame).
- Brand: the **waveform mark** is the logo/favicon; the **peacock (மயில்)** is the
  mascot (footer, 404, playground header). The Tamil மி lives in the wordmark text,
  not the favicon (an SVG favicon can't embed a Tamil font reliably).
