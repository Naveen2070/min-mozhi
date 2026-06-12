# Error fixtures — end-to-end failure validation

Each `.mimz` file here is **intentionally broken**. `tests/errors.rs` runs the
real `mimz` binary on every one and asserts it (a) exits non-zero and (b) prints
the declared stable error code. This is the end-to-end mirror of the in-process
checker unit tests in `src/checker/tests.rs`: those prove the checker _function_
rejects bad code; these prove the _CLI_ surfaces it, error code and all.

These files live under `tests/fixtures/` (not `examples/`) on purpose — every
file under `examples/` is asserted to be **valid** by `tests/examples.rs` and the
Icarus layer. Broken files must stay out of that tree.

## The convention

- **First line is the expectation:** `// expect: E0401` (regex
  `^//\s*expect:\s*(E\d{4})`). The runner reads it; no separate manifest.
- **Name encodes code + edge case:** `e0401_assignment_width.mimz`,
  `e0302_duplicate_conn.mimz`.
- **Parse-clean:** the file must lex + parse so the _checker_ runs and produces
  the target error — a syntax error would mask it. (Lexer/parser errors get
  their own coded fixtures once E10xx/E11xx land in Phase D.)
- **The assertion is "contains the code"**, not "only this error": a fixture that
  incidentally trips a second rule still passes as long as `error[<code>]`
  appears. Keep fixtures minimal so the target is the obvious failure.

## Adding one

1. Lift the broken snippet from the matching `src/checker/tests.rs` test (the
   test name encodes the code, e.g. `assignment_width_mismatch_is_e0401`).
2. Save it here with the `// expect:` header.
3. When a **new** checker E-code lands, add it to `ALL_CHECKER_CODES` in
   `tests/errors.rs` — the completeness guard then fails until a fixture exists.

Codes covered: every checker code in `docs/code/11-checker.md`
(E0001–E0701), each with a fixture per distinct edge case.
