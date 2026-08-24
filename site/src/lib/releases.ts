// Release data for the Downloads section.
//
// Populated by hand so builds stay hermetic — no runtime fetch from the GitHub
// API, which would make the site's content depend on a network call at build
// time. Append a new entry at the TOP when a version ships.

export interface ReleaseAsset {
  platform: string;
  arch: string;
  ext: string;
}

export interface Release {
  tag: string;
  name: string;
  date: string;
  assets: ReleaseAsset[];
}

export const REPO = "https://github.com/Naveen2070/min-mozhi";
export const RELEASES_URL = `${REPO}/releases`;

/** Every platform ships the same set; kept here so a new release is 4 lines. */
const STANDARD_ASSETS: ReleaseAsset[] = [
  {
    platform: "Linux (x86_64)",
    arch: "x86_64-unknown-linux-musl",
    ext: "tar.gz",
  },
  { platform: "macOS (Intel)", arch: "x86_64-apple-darwin", ext: "tar.gz" },
  {
    platform: "macOS (Apple Silicon)",
    arch: "aarch64-apple-darwin",
    ext: "tar.gz",
  },
  { platform: "Windows (x86_64)", arch: "x86_64-pc-windows-msvc", ext: "zip" },
];

/** Newest first. */
export const releases: Release[] = [
  {
    tag: "v0.2.0",
    name: "Wingless Butterfly",
    date: "2026-08-24",
    assets: STANDARD_ASSETS,
  },
  {
    tag: "v0.1.0",
    name: "Wingless Butterfly",
    date: "2026-06-24",
    assets: STANDARD_ASSETS,
  },
];

export const latest = releases[0];

/** Everything that is not the latest, newest first. */
export const history = releases.slice(1);

/** The few most recent past releases — what the sidebar lists. */
export const HISTORY_PREVIEW = 3;
export const historyPreview = history.slice(0, HISTORY_PREVIEW);

export function releaseByTag(tag: string): Release | undefined {
  return releases.find((r) => r.tag === tag);
}

export function isLatest(tag: string): boolean {
  return tag === latest.tag;
}

export function assetFilename(rel: Release, a: ReleaseAsset): string {
  return `mimz-${rel.tag}-${a.arch}.${a.ext}`;
}

export function assetUrl(rel: Release, a: ReleaseAsset): string {
  return `${REPO}/releases/download/${rel.tag}/${assetFilename(rel, a)}`;
}

export function checksumUrl(rel: Release): string {
  return `${REPO}/releases/download/${rel.tag}/SHA256SUMS`;
}

export function notesUrl(rel: Release): string {
  return `${RELEASES_URL}/tag/${rel.tag}`;
}
