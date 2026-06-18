# The Min-Mozhi Specification

The normative reference for Min-Mozhi (மின்மொழி) — what the language is, what the
compiler accepts, and why each rule exists.

If you want to _learn_ the language, start with the [guide](../docs/guide/README.md);
it teaches from the first module up. This specification is the reference the guide
is built on: it defines the grammar, the keyword table, the safety rules, and the
simulation model precisely. When the two disagree, the specification wins.

> Min-Mozhi describes **hardware**, not software. The grammar, the safety rules,
> and the type system all exist to make a circuit's behavior unambiguous at compile
> time. Read each document with that picture in mind and the constraints stop being
> surprising.

## Read in order

| #   | Document                                         | Defines                                                                                                 |
| --- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------- |
| 1   | [Goals & Philosophy](01-goals-and-philosophy.md) | What the language is for, who it serves, and the safety constitution it never breaks                    |
| 2   | [Syntax & Grammar](02-syntax-and-grammar.md)     | The full grammar — modules, types, expressions, statements — and the safety rules the compiler enforces |
| 3   | [Trilingual Keywords](03-keywords-trilingual.md) | One grammar, three keyword flavors (English, Tanglish, Tamil script) from a single table                |
| 4   | [Grammar Engine](04-grammar-engine.md)           | How the parser accepts natural Tamil (SOV) word order from the same grammar and AST                     |
| 5   | [Simulator](05-simulator.md)                     | The event-driven model behind `mimz sim` and `mimz test`, validated against Verilog                     |
| 6   | [Versioning & Editions](06-editions.md)          | The two version axes — compiler version and language edition — and how each advances                    |

Each document carries its own version and changelog. The keyword words themselves
live in [`../keywords.toml`](../keywords.toml); the compiler implements what is
written here.

## Where to start

Read [Goals & Philosophy](01-goals-and-philosophy.md) for the intent and the safety
guarantees, then [Syntax & Grammar](02-syntax-and-grammar.md) for the language
itself. The remaining documents are reference material for specific subsystems and
can be read in any order.

---

_This specification defines Min-Mozhi; the [guide](../docs/guide/README.md) teaches
it. The compiler in this repository is the implementation._
