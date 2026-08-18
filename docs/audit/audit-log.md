# Audit log — index

Every audit-driven change, newest last: what was found, what was done, and
what it cost. Split out of [`README.md`](README.md) — the README is the index
of the audit, this is the index of the ledger.

Each month is one file, and each entry is a `## <date> — <scope>` section
followed by its detail. It used to be a single table; one cell of it ran to
3000 characters, and a formatter pads every row of a table to its widest cell,
which is most of where the old 100 KB came from. **Append a new section to the
current month's file — do not put the ledger back in a table.**

| Month   | File                                           | Entries |
| ------- | ---------------------------------------------- | ------- |
| 2026-06 | [`audit-log/2026-06.md`](audit-log/2026-06.md) | 5       |
| 2026-07 | [`audit-log/2026-07.md`](audit-log/2026-07.md) | 12      |
| 2026-08 | [`audit-log/2026-08.md`](audit-log/2026-08.md) | 32      |
