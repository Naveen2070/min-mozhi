// Parse a lab lesson's markdown into its intro and `## Step N` sections.
//
// A lesson file (site/content/lab/*.md) is ordinary markdown: intro prose, then
// one `## Step N - Title` heading per step. Inside a step, three tagged code
// fences carry the exercise machinery (plan D2):
//
//   ```mimz starter     seeds the editor
//   ```mimz solution    revealed on request; also W6's build-time fixture
//   ```mimz verify      the hidden grader input appended by D3
//
// plus a `> hint:` blockquote. Everything else is learner-facing prose. Role
// fences are lifted OUT of the prose (they render only through the lab UI),
// while any other fenced block passes through untouched.
//
// Splitting at build time - the changelog.ts doctrine: the author writes plain
// markdown, the page renders pieces, nothing dynamic is invented per step.

export interface LabStep {
  /** The step number from the heading, e.g. 2 for "## Step 2 - Wire it". */
  n: number;
  /** Heading text after the number (may be empty). */
  title: string;
  /** Markdown prose, role fences and hints stripped, headings demoted one. */
  prose: string;
  starter?: string;
  /** Diagnostic the starter is SUPPOSED to hit (fence tag `fails E0502`);
      asserted by the W6 content gate, ignored by the UI. */
  starterFails?: string;
  solution?: string;
  verify?: string;
  /** Text of a `> hint:` blockquote, prefix stripped. */
  hint?: string;
}

export interface LabLesson {
  /** Markdown before the first step heading, headings demoted one. */
  intro: string;
  steps: LabStep[];
}

/** `## Step 2 — Wire the output` / `## Step 3: Clock it` / `## Step 4`. */
const STEP_HEADING = /^##\s+Step\s+(\d+)\s*(?:[-–—:]\s*)?(.*)$/;

type Role = "starter" | "solution" | "verify";
const ROLES: readonly Role[] = ["starter", "solution", "verify"];

function isFenceMark(line: string): boolean {
  return /^\s*(`{3,}|~{3,})/.test(line);
}

/** `​```mimz starter` opens a role fence; other fences are display code.
 *  A trailing `fails E0502` tags the starter's intended diagnostic (W6 gate). */
function roleOpen(line: string): { role: Role; fails?: string } | null {
  const m = line.match(
    /^\s*```+mimz\s+(starter|solution|verify)(?:\s+fails\s+([A-Z]\d{4}))?\s*$/,
  );
  return m ? { role: m[1] as Role, fails: m[2] } : null;
}

/**
 * Push every heading down one level: a step's own `###` becomes `####` so it
 * nests under the step heading the page supplies, keeping the outline honest.
 * Same rule as changelog.ts applies to release notes.
 */
function demote(md: string): string {
  return md.replace(/^(#{1,5})\s/gm, (_, hashes) => `${hashes}# `);
}

function trimBlank(lines: string[]): string[] {
  const out = [...lines];
  while (out.length && out[0].trim() === "") out.shift();
  while (out.length && out[out.length - 1].trim() === "") out.pop();
  return out;
}

/**
 * Split one step's body lines into prose / role fences / hint.
 *
 * A hint is a run of consecutive `>`-prefixed lines anywhere in the body;
 * the first non-quote line ends it. Only the FIRST hint per step wins - a
 * step makes one point, extra hints are an authoring mistake the W6 gate
 * should flag, not something to silently collect here.
 */
function extract(body: string[]): Omit<LabStep, "n" | "title"> {
  const proseLines: string[] = [];
  const roles: Partial<Record<Role, string[]>> = {};
  let hint: string[] | null = null;
  let capture: Role | null = null;
  let starterFails: string | undefined;
  let hintClosed = false;

  for (const line of body) {
    if (capture) {
      // The closing fence ends the capture; anything until then is content.
      if (isFenceMark(line)) capture = null;
      else roles[capture]!.push(line);
      continue;
    }
    const role = roleOpen(line);
    if (role) {
      capture = role.role;
      roles[role.role] ??= [];
      if (role.fails && role.role === "starter") {
        starterFails = role.fails;
      }
      continue;
    }
    if (!hintClosed && /^>\s?/.test(line)) {
      hint ??= [];
      hint.push(line.replace(/^>\s?/, ""));
      continue;
    }
    if (hint && line.trim() !== "") hintClosed = true;
    proseLines.push(line);
  }

  const step: Omit<LabStep, "n" | "title"> = {
    prose: demote(trimBlank(proseLines).join("\n")),
  };
  for (const r of ROLES) {
    const lines = roles[r];
    if (lines) step[r] = trimBlank(lines).join("\n");
  }
  if (starterFails) step.starterFails = starterFails;
  if (hint) step.hint = trimBlank(hint).join("\n");
  return step;
}

export function parseLesson(md: string): LabLesson {
  const lines = md.split(/\r?\n/);
  const introLines: string[] = [];
  const rawSteps: { n: number; title: string; body: string[] }[] = [];

  // Headings inside ANY fence are display code, not structure - track fence
  // state globally so a lesson showing markdown-in-a-block cannot invent steps.
  let inFence = false;

  for (const line of lines) {
    if (isFenceMark(line)) inFence = !inFence;
    const m = inFence ? null : line.match(STEP_HEADING);
    if (m) {
      rawSteps.push({ n: parseInt(m[1], 10), title: m[2].trim(), body: [] });
    } else if (rawSteps.length) {
      rawSteps[rawSteps.length - 1].body.push(line);
    } else {
      introLines.push(line);
    }
  }

  return {
    intro: demote(trimBlank(introLines).join("\n")),
    steps: rawSteps.map((s) => ({
      n: s.n,
      title: s.title,
      ...extract(s.body),
    })),
  };
}
