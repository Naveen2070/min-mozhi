import { readFileSync } from "node:fs";
import path from "node:path";

// Reader for lang/keywords.toml — the compiler's source of truth for keyword
// spellings. The Handbook's keyword table is generated from this at build time,
// so the page cannot drift from what the compiler actually accepts.
//
// This is NOT a general TOML parser. It understands exactly the shapes that file
// uses: two top-level keys (`version`, `reserved`) and flat `[keywords.<name>]`
// tables holding `en` / `tanglish` / `tamil` plus optional `<col>_aliases` arrays.
// Written as .mjs (like rehype-doc-links.mjs alongside it) so the accompanying
// test runs under plain `node`, with no build step.

// Counts as of keyword-set v1. Asserted at load so a drift in keywords.toml
// fails the site build loudly instead of silently changing a published page.
const EXPECTED_KEYWORDS = 44;
const EXPECTED_RESERVED = 12;

/**
 * Split `rest` at the first `#` that is outside a quoted string.
 * @returns {{ value: string, trailing: string }}
 */
function splitValue(rest) {
  let inString = false;
  let i = 0;
  for (; i < rest.length; i++) {
    const c = rest[i];
    if (c === '"') inString = !inString;
    else if (c === "#" && !inString) break;
  }
  return { value: rest.slice(0, i).trim(), trailing: rest.slice(i) };
}

/** Pull the string items out of a single-line TOML array: `["a", "b"]`. */
function parseInlineArray(value) {
  return [...value.matchAll(/"([^"]*)"/g)].map((m) => m[1]);
}

/**
 * @typedef {object} Keyword
 * @property {string} name        table key, e.g. "module"
 * @property {string} en
 * @property {string} tanglish
 * @property {string} tamil
 * @property {Record<string, string[]>} aliases  e.g. { en: ["include"] }
 * @property {boolean} provisional  spelling is a placeholder pending native review
 */

/**
 * @param {string} text contents of keywords.toml
 * @returns {{ version: number|null, keywords: Keyword[], reserved: string[] }}
 */
export function parseKeywords(text) {
  const lines = text.split(/\r?\n/);

  /** @type {number|null} */ let version = null;
  /** @type {string[]} */ const reserved = [];
  /** @type {Keyword[]} */ const keywords = [];

  // Comment lines seen since the last blank line. A block comment sits ABOVE the
  // table it describes, so this buffer belongs to the table we have NOT reached
  // yet. Attributing it to the table above instead — the obvious-looking read —
  // names the wrong keyword every single time.
  let comment = "";
  /** @type {Keyword|null} */ let current = null;
  let inReservedArray = false;

  const flush = () => {
    if (current) keywords.push(current);
    current = null;
  };

  for (const raw of lines) {
    const line = raw.trim();

    if (inReservedArray) {
      if (line.startsWith("]")) {
        inReservedArray = false;
      } else if (!line.startsWith("#")) {
        // Take only the first quoted token: entries may carry a trailing comment.
        const m = line.match(/"([^"]*)"/);
        if (m) reserved.push(m[1]);
      }
      continue;
    }

    // A blank line ends a comment block, so an unrelated block above does not
    // bleed into the next table.
    if (line === "") {
      comment = "";
      continue;
    }

    if (line.startsWith("#")) {
      comment += line + "\n";
      continue;
    }

    const table = line.match(/^\[keywords\.([^\]]+)\]/);
    if (table) {
      flush();
      current = {
        name: table[1],
        en: "",
        tanglish: "",
        tamil: "",
        aliases: {},
        provisional: /PROVISIONAL/.test(comment),
      };
      comment = "";
      continue;
    }

    // Any other table header ends the current keyword.
    if (line.startsWith("[")) {
      flush();
      comment = "";
      continue;
    }

    const kv = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/);
    if (!kv) continue;
    const [, key, rest] = kv;
    const { value, trailing } = splitValue(rest);

    // PROVISIONAL also appears as a trailing comment on individual value lines.
    if (current && /PROVISIONAL/.test(trailing)) current.provisional = true;

    if (!current) {
      if (key === "version") version = parseInt(value, 10);
      else if (key === "reserved") {
        if (value.includes("]")) reserved.push(...parseInlineArray(value));
        else inReservedArray = true;
      }
      continue;
    }

    if (key.endsWith("_aliases")) {
      current.aliases[key.replace(/_aliases$/, "")] = parseInlineArray(value);
    } else if (key === "en" || key === "tanglish" || key === "tamil") {
      current[key] = value.replace(/^"|"$/g, "");
    }
  }

  flush();
  return { version, keywords, reserved };
}

/**
 * Read + validate lang/keywords.toml.
 *
 * Deliberately asserts nothing about how many keywords are PROVISIONAL: that set
 * shrinks as native-speaker review lands, and pinning it would turn routine
 * linguistic progress into a red site build.
 */
export function loadKeywords(repoRoot = path.resolve(process.cwd(), "..")) {
  const file = path.join(repoRoot, "lang", "keywords.toml");
  const table = parseKeywords(readFileSync(file, "utf-8"));

  if (table.keywords.length !== EXPECTED_KEYWORDS) {
    throw new Error(
      `keywords.toml: expected ${EXPECTED_KEYWORDS} keywords, parsed ${table.keywords.length}. ` +
        `If the keyword set really changed, update EXPECTED_KEYWORDS in src/lib/keywords.mjs ` +
        `and re-check the Handbook keyword chapter.`,
    );
  }
  if (table.reserved.length !== EXPECTED_RESERVED) {
    throw new Error(
      `keywords.toml: expected ${EXPECTED_RESERVED} reserved words, parsed ${table.reserved.length}.`,
    );
  }

  return table;
}
