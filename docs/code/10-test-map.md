# 10 — Test Map: What Is Covered, What Isn't, and Why

Every test, what it locks in, and what a failure means. Update this page
when tests are added or removed (the count below is asserted nowhere —
this page is the human ledger).

> **Live breakdown:** run **`cargo test-summary`** instead of `cargo test` — it
> runs the suite, then prints a per-binary table (lib unit, each bin, every
> integration suite, doctests) and a grand total. Cross-platform (a standalone
> dev crate at `tools/test-summary/`, aliased in `.cargo/config.toml`); forwards
> all `cargo test` args (`--release`, `--test sim`, …) and honors
> `REQUIRE_IVERILOG`. Use it to keep the hand-maintained counts above honest.

**469 tests** as of 2026-06-25: 315 lib unit + 7 LSP unit (bin) + 6 benchmark unit (bin) + 13 example integration + 16 grammar integration + 10 eval integration + 14 translate integration + 20 morph integration + 9 fmt integration + 5 Icarus differential + 4 error-fixture + 1 LSP smoke + 4 docs-sync + 6 grammar-sync + 5 config integration + 5 compile_string integration + 10 sim integration + 7 test integration + 11 stdlib integration + 1 wasm_parity integration.

Fixture counts (current): 72 error fixtures · 8 grammar fixtures · 41 golden `.v` outputs + 14 `_tb.v` testbench goldens + 1 `.vcd` · 32 Icarus self-checking testbenches.

Changelog of test-count changes (newest first):

- 2026-06-25 LSP DX (branch `phase-4-lsp-dx`). `mimz lsp` serves hover (type + doc-on-type), go-to-definition (cross-file, `test` blocks, `import` targets), and completion (scope identifiers + flavor keywords). `src/analysis.rs`: symbol index, `resolve_at` offset-to-definition resolver, completions — scope idents + flavor keywords. `src/lsp.rs`: LSP server wired through Tower LSP. `KeywordTable::canonical_spellings` for flavor-aware keyword completion. **+12 lib unit** (analysis.rs: symbol index, resolve_at, completions; lsp.rs: handlers; tests for each) **+1 LSP unit (bin)** (`lsp.rs` smoke). Suite 456 → 469.

- 2026-06-24 Importable `std.*` library (branch `stdlib-importable-path`). `import std.fifo` (and `serkka nuulagam.varisai` / `சேர்க்க நூலகம்.வரிசை`) now resolve to an **embedded** standard library — `src/stdlib.rs` `include_str!`s the already-tested `examples/english/std/*.mimz` + `examples/tamil-pure/*.mimz` (zero duplication), so resolution needs no install path and works in WASM. Routing keys on the written alias: English stem → canonical module, twin name/romanization → pure-Tamil twin. `src/project.rs` gained a `std` branch (`load_project_with_lib`) that parses the embedded `&str` into a synthetic in-memory file, or loads `<dir>/<m>.mimz` when `mimz.toml [lib] std` overrides; `src/config.rs` gained the `[lib]` section + `resolve_with_path`; `mimz eject std` (`src/commands/eject.rs`, `stdlib::eject_to`) vendors the library all-or-nothing. New loader code **E1202** (bad std import) added to `src/explain.rs` + `06-diagnostics.md`. **+8 lib unit** (5 in `src/stdlib.rs`: aliases, canonical/twin routing, unknown-module, no-transitive-imports invariant; 3 in `src/config.rs`: `[lib]` parse, unknown-key reject, `resolve_with_path` location) **+11 stdlib integration** (new `tests/stdlib.rs`: embedded resolve + entry-stays-`files[0]` ordering, Tamil twin routing, unknown/arity E1202, relative-import regression, `[lib]` override wins + twin-spelling override matches eject, 3 eject + all-or-nothing partial-conflict). Spec/02 §1.5 gained the `std.*` clause. A post-review fix corrected two bugs the green suite missed: embedded std modules were pushed ahead of the entry (breaking the `files[0] == entry` invariant `sim`/`test` rely on), and the `[lib]` override keyed the filename on the raw written alias instead of the resolved variant (so a Tamil-twin-name import missed the ejected `varisai.mimz`). Suite 455 → 456.

- 2026-06-23 BUG-6 (left-shift truncation) fixed in `src/sim/value.rs`. +1 lib unit (`shl_does_not_truncate_to_left_operand_width`). The shift example (`examples/english/shift.mimz`) was rewritten to follow the template (header + inline tests), mixed flavor added, and a real pure-Tamil twin `tamil-pure/nakartthi.mimz` created (replacing the old `shift.mimz` which had English identifiers). Both registered: `BASE_EXAMPLES` 28 → 29, `PURE_TAMIL` 12 → 13 (`tests/examples.rs`); `nakartthi` added to the `tests/icarus.rs` differential. The FIFO workaround (explicit `DEPTH` param) was reverted — all 4 flavors + `varisai` now use `1 << AW`. The FIFO doc page was updated accordingly (removed `DEPTH` parameter row). **No new test functions** beyond the shl unit test — the example and the revert ride the existing parametrized loops. Suite count 436 → 437.

- 2026-06-23 stdlib modules `seg7`, `pwm`, `fifo`, `uart_tx` shipped (after `debouncer`), each in all four flavors + a pure-Tamil twin (`ennkaatti`, `minukki`, `varisai`, `anuppi`), with inline `test` blocks, module + emitted-testbench goldens, and a hand-written self-checking Icarus testbench. **No new test functions** — the modules ride the existing parametrized loops, so `BASE_EXAMPLES` 24 → 28, `PURE_TAMIL` 8 → 12 (`tests/examples.rs`) and `TESTBENCHES` 17 → 21, `PURE_TESTBENCHES` 7 → 11 (`tests/icarus.rs`) auto-extend coverage. Suite count unchanged at 436.

- 2026-06-22 Parser AST error recovery (`phase-4-parser-ast-error-recovery` branch; Phase 4 LSP prerequisite, `architectural_ideas.md` idea 1). New `Error(Span)` variant on `TopItem`/`ModuleItem`/`SeqStmt`/`TestStmt` + a non-discarding `parser::parse_recover` entry point that leaves an `Error` placeholder at each recovery boundary instead of dropping the broken construct (the strict `parse` is unchanged — any error still discards the tree, so codegen never sees an `Error` node). Every consumer handles the variant (checker skips, codegen treats as unreachable). +4 lib unit (`parser`: `parse_recover_keeps_good_items_around_a_bad_one`, `parse_recover_top_level_error_keeps_following_module`, `parse_recover_seq_and_test_blocks_emit_error_nodes`, `strict_parse_still_errs_on_bad_input`). Suite 432 → 436.
- 2026-06-22 Fuzz crash fix: `is_word_byte` was missing `?`, so `push_guarded` in `translate::reskin` didn't insert a separating space when a `MaskedInt` ending with `?` (e.g. `0b1?`) abutted a romanized identifier, causing the re-lexer to consume `0b1?rrrram` as a single invalid number. +2 lib unit (`masked_int_q_does_not_glue_onto_romanized_identifier`, `masked_int_q_does_not_glue_onto_english_keyword`). Also: rebuilt `crates/mimz-wasm/pkg/` with `--target nodejs` + fixed `pkg/package.json` `"type": "commonjs"` — `wasm_parity` now passes locally on Node 24 (was a pre-existing ESM/CJS interop failure). Site `npm run build` auto-runs `build:wasm` to regenerate the web glue. Suite 430 → 432.

- 2026-06-22 Reserved `extern` (external-Verilog / black-box-IP module; `docs/Ideas/architectural_ideas.md` idea 3) ahead of the v0.1.0 freeze (R11): added to `lang/keywords.toml` `reserved` + spec/03 v0.2.11 + the grammar invalid pattern + a lexer test. The three separate reserved-word keyword-table tests (`fn_and_function_are_reserved`, `the_v03_backlog_keywords_are_reserved`, `the_section8_keywords_are_reserved`) were merged into one data-driven `future_keywords_are_reserved_not_usable` that also covers `extern`. Net −2 lib unit (3 removed, 1 added). Suite 432 → 430.

- 2026-06-22 WASM↔CLI Verilog parity + testbench golden/Icarus coverage. New `tests/wasm_parity.rs` asserts the `mimz-wasm` `compileToVerilog` binding emits byte-identical Verilog to the CLI's `compile` — the CLI writes to a temp `-o` path the test reads then deletes (cleaned up even if the assertion fails), so the comparison is file-content vs binding output, not status-line vs Verilog; skips with a note when `crates/mimz-wasm/pkg/` isn't built. The `--emit-testbench` work also landed `emitted_testbench_matches_the_goldens` + `emit_testbench_without_test_blocks_notes_and_writes_only_v` (`tests/examples.rs`) and `every_emitted_testbench_passes_iverilog` (`tests/icarus.rs`). +2 example integration, +1 Icarus differential, +1 wasm_parity integration. Suite 428 → 432.
- 2026-06-21 Testbench emitter (`src/emit_verilog/testbench.rs`) `--emit-testbench` fixes: `test_env` now merges the DUT's module-parameter defaults for any arg a test doesn't override (mirrors `sim::elaborate::elaborate_module`'s override-or-default order), and args chain left-to-right so a later arg can reference an earlier one (mirrors `sim::harness::params`) — without this, a defaulted param omitted by a test, or `M(W: 8, DEPTH: W * 2)`-style chaining, failed to resolve width expressions. Also: two tests whose names sanitize to the same Verilog module identifier (e.g. `"edge case"` and `"edge_case"` both → `edge_case_tb`) are now rejected with a diagnostic instead of silently emitting two same-named modules. +3 lib unit (`test_env_falls_back_to_module_param_defaults`, `test_env_chains_earlier_args`, `colliding_sanitized_test_names_are_rejected`). Suite 425 → 428.
- 2026-06-21 Testbench emitter (`src/emit_verilog/testbench.rs`) security and logic hardening — added `sanitize_verilog_ident` helper, bounded loop iteration counts, properly recursed into nested conditionals within inline tests, and pushed `consteval` errors gracefully. +1 lib unit (`sanitize_verilog_ident_replaces_invalid_chars`). Suite 424 → 425.
- 2026-06-20 Re-audit `src/sim/value.rs`: Finding A — `BinOp::Shl` used bare `r.bits as u32` to cast the shift amount, silently truncating when bit ≥ 32 was set (e.g. `1 << (1 << 32)` became `1 << 0` = 1 instead of 0). Also corrected `BinOp::Shr`'s `.min(127)` guard which avoided the truncation panic but produced wrong results (shift-by-128 became shift-by-127 instead of 0). Both fixed with `if r.bits >= 128 { 0 } else { … as u32 }`. +7 lib unit in `sim::comb::tests` (all new, section below). Suite 417 → 424.
- 2026-06-19 Two new pure-Tamil showcase examples so the playground's six curated examples (counter, adder, comparator, mux4, blinker, traffic*light) exist in **every** flavor — `examples/tamil-pure/kuutti.mimz` (adder twin) and `saalaivilakku.mimz` (traffic-light FSM twin), both Tamil keywords AND identifiers. `PURE_TAMIL` (in `tests/examples.rs` and `tests/translate.rs`) grew 4 → 6, so the equivalence, golden, and round-trip checks now cover them (new goldens `tests/golden/tamil_pure*{kuutti,saalaivilakku}.v`); the Icarus suite gained matching self-checking testbenches (`tests/icarus/{kuutti,saalaivilakku}\_tb.v`) + bit-for-bit differentials. **No new `#[test]` functions\*\* (these ride existing loop-driven tests), so the count is unchanged at 417.
- 2026-06-19 Website Phase 4 — the interactive playground waveform. The runner (`src/runner.rs`) gained a `ports` command (emits the module interface as JSON — `{module, clocked, inputs[], outputs[]}` — so the browser can build input controls without re-parsing) and a `sim --steps "a=3,b=5;a=7,b=1"` flag (explicit per-step input vectors, fed straight into the existing `comb_run`; rejected for clocked designs). The `/playground` got a stimulus panel — an editable step table for combinational designs (the fix for "an adder with a fixed input draws flat") and held-inputs + cycles for clocked ones — that re-simulates live, plus a hover cursor on the canvas reading each signal's value at a time point. +4 lib unit (`runner`: ports×2, sim_steps×2). Suite 413 → 417.
- 2026-06-18 Website Phase 4 step 5 — the playground waveform viewer. The runner's `sim` gained a `--vcd` flag (returns the 2-state VCD from `sim::vcd::to_vcd` instead of a console trace), so the in-browser **Simulate** button gets a waveform via the existing `runCommand` (no new wasm binding). New `site/src/components/WaveformViewer.tsx` — a self-contained canvas renderer behind the stable `vcd` prop (parses the VCD; square waves for 1-bit, value-labelled buses for wider signals; Surfer is the documented future drop-in). +1 lib unit (`runner::sim_vcd_emits_a_vcd_document`). Suite 412 → 413.
- 2026-06-18 Website Phase 4 step 4 — the in-browser playground console. New `src/runner.rs` (private lib module, re-exported): a filesystem-free `run_command(source, command, argv)` that runs `check`/`compile`/`eval`/`sim`/`test` against a source string and returns the text a user would see, composing the existing lib pipeline (`comb::eval_outputs`, `elaborate`, `run`/`comb_run`, `trace::render`). The `--in`/`--param`/`--sweep`/trace-scope parsers were **lifted from the CLI's `commands/helpers.rs` into the lib** (single source; the CLI now re-exports them), and `compile_string` is now a thin wrapper over `run_command`. The wasm crate gained `runCommand`; the site got a `/playground` page (textarea editor + console, a `client:only` React island over the web wasm). +5 lib unit (`runner`: sweep×2, check, eval, sim), −2 command unit (the moved `sweep_vectors` tests). Suite 409 → 412.
- 2026-06-18 Website Phase 2 (WASM groundwork) — `mimz::compile_string` (`src/lib.rs`): the filesystem-free `lex→parse→check→transliterate→emit` entry point behind the browser playground (single-file; `import` rejected with a plain message). New `crates/mimz-wasm` (wasm-bindgen `compileToVerilog`) + a Cargo workspace; the CLI-only deps (`tokio`/`tower-lsp`/`memory-stats`) were made optional and feature-gated (`default = ["lsp", "bench"]`) so the lib builds for `wasm32` under `default-features = false`. +5 compile_string integration (`tests/compile_string.rs`: valid compile names the module, trilingual byte-identical output, E0401 width mismatch, syntax error reported, `import` rejected). Verified: full native gate green, `cargo build -p mimz-wasm --target wasm32-unknown-unknown`, and a headless Node smoke test (`crates/mimz-wasm/smoke-test.cjs`) compiling the counter through wasm. Suite 404 → 409.
- 2026-06-17 Workstream B versioning + language edition — new `src/version.rs`: the compiler-version vs language-edition axes, `EDITION_HISTORY` (first edition **Wingless Butterfly** `wingless-butterfly-2026-1`), `version_block()` (uname-style `mimz --version`), and `KEYWORD_SET_VERSION` cross-checked against `lang/keywords.toml`'s `version` (now parsed + exposed via `KeywordTable::version`). The Verilog header carries both axes. +3 lib unit (`version`: `current_is_the_last_history_row`, `keyword_set_version_matches_keywords_toml`, `version_block_shows_both_axes`). Crate stays `0.1.0-dev` (drops `-dev` at the v0.1.0 tag, Workstream D). Suite 401 → 404.
- 2026-06-17 A5 asynchronous reset `async reset` (pre-v0.1.0 RTL-parity batch) — `async` promoted from reserved to an active keyword KW_ASYNC (Tanglish/Tamil `otthisaivatra`/`ஒத்திசைவற்ற` PROVISIONAL, pending native review). `ModuleItem::Reset` became `{ name, is_async }`; the emitter widens the sensitivity list to `@(posedge clk or posedge rst)` for an async reset. Active-high only (active-low polarity deferred). The cycle-based kernel is unchanged — async and sync reset are observationally identical at per-cycle sample points, so it's an emitter-only distinction. +5 lib unit (lexer `async_is_an_active_keyword`; parser `async_reset_parses_with_the_async_flag`, `a_plain_reset_is_synchronous`; emitter `async_reset_widens_the_sensitivity_list`, `a_sync_reset_stays_clock_only`). New four-flavor `async_reset` example (`BASE_EXAMPLES` 21 → 22, golden + the Icarus three-way differential). Spec `02` → v0.2.12, `03` → v0.2.10. Suite 396 → 401.
- 2026-06-17 A4 memories `mem` (pre-v0.1.0 RTL-parity batch) — `mem` promoted from reserved to an active keyword KW_MEM (Tanglish/Tamil `ninaivagam`/`நினைவகம்` PROVISIONAL, pending native review). New `ModuleItem::Mem`; checker `Ty::Memory` (indexed read/write yields the element type, address range-checked against `depth`); emitter `reg [W-1:0] m [0:DEPTH-1]` + an `initial` power-on seed; the sim kernel gained a sparse cell store (`is_mem`/`mem_read` on the `Resolver`, indexed write into `next_mems`). +10 lib unit (lexer `mem_is_an_active_keyword`; parser `mem_declaration_parses_to_a_mem_item`, `a_mem_without_an_init_value_is_e1104`; checker `register_file_passes`, `a_non_constant_memory_depth_is_e0201`, `a_zero_memory_depth_is_e0410`, `a_memory_init_that_overflows_the_element_is_e0405`, `a_constant_address_past_the_depth_is_e0406`, `a_memory_inside_repeat_is_e0303`; kernel `memory_write_then_read_round_trips_a_cell`). New four-flavor `regfile` example (`BASE_EXAMPLES` 20 → 21, golden + the Icarus three-way differential; the `regfile` cells are internal-only — not dumped to VCD, like the tamil-pure exemption note). Spec `02` → v0.2.11, `03` → v0.2.9. Suite 386 → 396.
- 2026-06-17 A3 falling-edge `on fall(clk)` (pre-v0.1.0 RTL-parity batch) — `fall` promoted from reserved to an active keyword KW_FALL (Tanglish/Tamil `irakkam`/`இறக்கம்` PROVISIONAL, pending native review); `OnBlock`/`Reg`/`Process` gained an `edge`; emitter lowers `posedge`/`negedge`; the sim kernel is now edge-aware (rise → sample → fall per period) so mixed-edge designs match Icarus bit-for-bit. +4 lib unit (parser `on_fall_parses_with_the_fall_edge`, `thamizh_order_on_fall_parses_to_the_fall_edge`; emitter `on_fall_emits_negedge`; kernel `dual_edge_negedge_reg_captures_posedge_within_a_period`); 2 lexer tests renamed (`fall_is_an_active_keyword`, `a_reserved_word_is_an_error`). New four-flavor `dual_edge` example (`BASE_EXAMPLES` 19 → 20, golden + the Icarus three-way differential). Spec `02` → v0.2.10, `03` → v0.2.8. Suite 382 → 386.
- 2026-06-17 A2 don't-care `match` patterns `0b1??` (pre-v0.1.0 RTL-parity batch) — new `TokKind::MaskedInt` / `Pattern::IntMask` (binary `?` don't-care), mirroring the literal-pattern path; additive, no new keyword. +6 lib unit (lexer `dont_care_binary_literal_lexes_to_masked_int`; parser `dont_care_pattern_parses_to_intmask`; checker `dont_care_pattern_must_match_the_scrutinee_width`, `a_dont_care_match_still_needs_a_wildcard`, `a_dont_care_pattern_on_an_enum_is_e0409`; sim `dont_care_match_picks_the_masked_arm`). New four-flavor example `priority` (`BASE_EXAMPLES` 18 → 19, golden + the Icarus three-way differential) — no new test functions. Exact-width reuses E0409, still-needs-`_` is E0601 (no new code). Spec `02` → v0.2.9. Suite 376 → 382.
- 2026-06-17 A1 replication `{N{x}}` (pre-v0.1.0 RTL-parity batch) — new `ExprKind::Replicate` mirroring concat through the whole pipeline; purely additive, no new keyword. +7 lib unit (parser `replication_parses_to_replicate`, `braces_without_an_inner_group_stay_concat`; checker `replication_width_is_count_times_inner`, `replication_width_mismatch_is_e0401`, `a_non_constant_replication_count_is_e0201`, `a_zero_replication_count_is_e0410`; sim `replication_repeats_the_group`). New four-flavor example `replicate` (`BASE_EXAMPLES` 17 → 18, golden + the Icarus three-way differential) — no new test functions (existing parametrized iterators). Width reuses E0410, non-const count reuses E0201 (no new code). Spec `02` → v0.2.8. Suite 369 → 376.
- 2026-06-17 SEC-6 hardening audit — C2–C4 elaboration-time DoS bounds: `mimz sim`/`mimz test` skip the checker, so the structural elaborator (`src/sim/elaborate.rs`) gained `MAX_INSTANCE_DEPTH = 16` (recursive/cyclic instantiation → clean error, not a stack-overflow abort), `checked_sub` on the `repeat` span (extreme `hi - lo` → over-budget error, not an overflow panic), a `0..128` bound on bit-index drives (no silent `as u32` truncation), and a flatten name-collision error (no silent overwrite). A same-day follow-up pass added a 5th finding (SIM-5): `int_expr`, which lowers each flattened child const to a literal, built a negative value via a raw `i128` negation that overflow-panicked on `i128::MIN` (reachable via `(-i128::MAX) - 1`) — now non-recursive and `unsigned_abs`-based. +5 lib unit (`recursive_instantiation_errors_not_overflows`, `extreme_repeat_bounds_error_not_overflow`, `an_out_of_range_bit_index_errors`, `a_flatten_name_collision_errors`, `an_i128_min_const_elaborates_without_overflow` — `src/sim/elaborate.rs`). See SEC-6/HARD-6 in `docs/audit/`.
- 2026-06-16 Phase 1.5 C3 + C4 — full simulator parity: the sim elaborator now unrolls `repeat` (array instances `fa__i`, bit-indexed drives assembled into a Concat — ripple\*adder) and encodes enum-typed signals by variant index with width `clog2(variants)` (variant reads/patterns → index — traffic_light), via a unified `Rw` elaborate-time rewriter (`src/sim/elaborate.rs`). The Layer-3 differential now covers the **entire single-file corpus, 18 → 21 examples** (added ripple_adder, traffic_light, vilakku) — every example the emitter compiles also simulates bit-for-bit vs Icarus. +2 lib unit (`unrolls_repeat_with_instance_array_and_bit_drives`, `elaborates_an_enum_signal_and_match`). Phase 1.5 full-parity simulator complete (C1–C4).
- 2026-06-16 Phase 1.5 C2 — module-instance flattening in the sim elaborator: `elaborate_project` (`src/sim/elaborate.rs`) flattens `let` instances (incl. across `import`s) by inlining each child with signals name-prefixed `{inst}*{name}`, so `inst.port`reads resolve to the wire`inst*port`the emitter auto-declares — the flattened`Design`matches the emitted Verilog bit-for-bit.`mimz sim`/`mimz test`now`load_project`; the Layer-3 differential gained **alu** (`Top`instantiating the imported`Adder`) and **chained** (two chained `FullAdder`s), 16 → **18 examples**. +2 lib unit (`flattens_a_same_file_instance`, `rejects_unknown_instance_module`, replacing `rejects_instances_for_now`); the differential is one `#[test]`so the new examples add no separate count. Remaining sim parity: C3`repeat`(ripple_adder), C4 enum FSM (traffic_light).
- 2026-06-16 security/bug audit (SEC-5) — bound the simulator's unbounded count inputs: a critical→medium audit (core pipeline clean) found the new sim skipped the "bound every count" doctrine. Caps`MAX_SIM_CYCLES`/`MAX_SWEEP_VECTORS` (`src/sim/run.rs`) now bound `tick(clk, n)`(untrusted-input hang/OOM via`mimz test`), the `--sweep`cartesian product (unchecked`usize`mul), and`--cycles`; plus a `translate`no-panic fix and a`mimz.toml` walk-up cap. +2 command unit (`sweep_vectors`cap —`src/commands/helpers.rs`), +1 sim integration (`cycles_over_the_limit_is_rejected_by_the_cli`), +1 test integration (`a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`). The auditor's `cycle * PERIOD`overflow "highs" are unreachable once the loops are bounded — recorded checked-safe, see`docs/audit/`.
- 2026-06-16 C1 carry-forward closed — the Layer-3 Icarus differential (`our*simulator_matches_icarus_bit_for_bit`) now also covers the four pure-Tamil examples (kanakki/cimitti/oppidi/thervi), so its list equals the emitter's single-module list, **12 english + 4 tamil-pure = 16**. The testbench romanizes interface names via the emitter's own `transliterate` (`interface_name_map`in`tests/icarus.rs`) to match the compiled Verilog while the kernel keeps source names; no new test function, so the count is unchanged.
- 2026-06-16 Phase 1.5 C1 — combinational `mimz sim`+ signed-aware differential:`comb_run` (`src/sim/run.rs`) settles a clockless design one frame per input vector, so `mimz sim`now runs combinational modules too —`--in`is one settled frame,`--sweep a=0|1|2` a frame each — emitting the same VCD/trace. The Layer-3 Icarus differential (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) was broadened to **12 ASCII-named english examples** (clocked AND combinational, incl. SIGNED `bitops`/`signed_math`), auto-routing on whether the design is clocked, comparing via Verilog `%b`(binary ⇒ signedness-agnostic) with per-example param overrides. It caught a real bug: the shared evaluator's lossless signed`+`/`*` (`src/sim/value.rs`) added raw bits without sign-extending a negative operand — fixed to use `as_i128`(matching Verilog), which also corrects`mimz eval`. +5 lib unit (4 `comb_run` + 1 signed regression) + 2 net sim integration (−1 clockless-reject removed, +3 combinational). Romanized tamil-pure + instance/`repeat`/enum designs are deferred (C2–C4).
- 2026-06-16 Phase 1.5 B8 — differential vs Icarus + perf baseline + golden VCD: a Layer-3 Icarus test (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) runs each design through OUR event-driven kernel in-process AND reconstructs the values from the VCD our writer emits, comparing both against `iverilog`/`vvp` under the SAME stimulus — three views (kernel == VCD waveform == Icarus) must agree bit-for-bit per cycle (counter + shift register + edge detector). A byte-for-byte golden lock (`tests/sim.rs::the_counter_vcd_matches_the_golden_byte_for_byte`vs`tests/golden/counter.vcd`, `MIMZ_UPDATE_GOLDENS=1` to regenerate) pins the writer's exact output format. A perf test (`tests/sim.rs::the_counter_kernel_clears_the_perf_baseline`) gates the kernel at ≥1M cycle-events/sec on the counter in release (best of 5 to reject load-induced dips; measured ~2.3M; debug uses a low sanity floor). +1 Icarus differential + 2 sim integration. Phase 1.5 (simulator) is now feature-complete: B1 elaborate, B2 kernel, B3 comb propagation, B4 stimulus, B5 VCD+trace+`mimz sim`, B6 `mimz test`, B7 test-header flip, B8 differential+perf+golden.
- 2026-06-16 Phase 1.5 B7 — test-header thamizh-order flip: `M(args) kaaga "…" sodhanai { }`parses to the SAME`TestDecl`as the code-order`test "…" for M(args) { }` (`src/parser/items/test.rs::test_decl_thamizh`, dispatched from the file loop when `syntax thamizh`is active and a bare identifier leads), and`src/pretty.rs`flips it for`mimz translate --order thamizh`— completing all five clause flips of the word-order engine. Execution is the oracle: a passing thamizh-order test re-parsing to the same tree replaces the same-Verilog check`test` blocks can't provide. +3 parser lib unit + 1 test integration (`a_thamizh_order_test_header_runs_like_its_code_order_twin`) + 1 translate integration (`pretty_print_thamizh_flips_the_test_header_and_reparses`).
- 2026-06-16 Phase 1.5 B6 — `mimz test`: the `test`-block runner in `src/sim/harness.rs` runs each block (`drive`/`tick`/`expect`/`if`) on the kernel, halts a failing `expect`with a teaching message (expression source + cycle + each comparison side's value), and exits non-zero on any failure;`--filter`/`--trace`/`--verbose`/`--signals`supported, the trace-scope logic shared with`mimz sim`via`commands/helpers.rs::trace_scope`. `async`was reserved alongside`await` (spec/03 v0.2.7, R11/R13) so the v0.3 backlog list is now 9 words. +6 lib unit (`src/sim/harness.rs`) + 5 test integration (`tests/test_run.rs`).
- 2026-06-16 Phase 1.5 B4+B5 — `mimz sim`: default stimulus + a hand-written 2-state VCD writer + the `--trace`/`--trace=changes`console table (scope via`--verbose`/`--signals`), all riding one per-cycle snapshot from the kernel. +9 lib unit (`src/sim/{run,vcd,trace}.rs`) + 5 sim integration (`tests/sim.rs`).
- 2026-06-16 Phase 1.5 B1 — simulator elaboration: +5 lib unit in `src/sim/elaborate.rs`, the `Design`flattener (signals/regs/comb/processes, widths + reset folded) the event-driven kernel will interpret.
- 2026-06-16 Phase 1.5 B2 — event-driven two-phase kernel: +7 lib unit in`src/sim/kernel.rs` (counting/reset, width-wrap, the two-phase register swap, statement-`if`, the per-cycle snapshot seam, leaf validation). The shared 2-state value model + expression evaluator were extracted to `src/sim/value.rs`behind a`Resolver`trait that both`comb`and`kernel`implement —`comb`'s 7 tests are unchanged and verify the extraction.
- 2026-06-16 Phase 1.5 B3 — combinational propagation: +2 kernel lib unit locking multi-level `wire → wire → output`settling order and the kernel's comb-cycle guard; B3 needed no new code — the kernel's memoized resolver already settles drivers in dependency order.
- 2026-06-16 close Phase 1.8 + pre-freeze keyword reservation: Phase 1.8 closed by bumping`spec/04`DRAFT → stable (docs only, no test change); and`fn`/`function`reserved for a future combinational-function construct ahead of the v0.1.0 freeze (R11/R13) — +1 keyword-table lib unit`fn_and_function_are_reserved`. Also listed `the_section8_keywords_are_reserved` in the keyword-table section below, present since 2026-06-13 but previously unlisted.
- 2026-06-16 native-authored error catalog + audit/coverage follow-up: the Tamil/Tanglish catalog (`lang/messages.toml`, decision C3 ratified) grew from a one-shape stub to **33 of 36** localized codes with structured-arg interpolation; an audit of PRs #14–#17 found no bug/overflow/security/perf issue, so the work was test-coverage + prevention guards only. +2 morph lib unit (`arg_code_without_args_falls_back_to_english`, `fill_with_empty_name_leaves_no_stray_fragment`), +4 morph integration (`e0402`/`e0408`/`e0601`interpolation tests +`message_catalog_placeholders_are_known_tokens`— a guard that every active`{token}`in`lang/messages.toml`is one`morph::fill` fills, so a typo'd placeholder can't silently fall back to English forever), +1 grammar-sync (`keywords_toml_has_no_superseded_spelling` — a superseded v1 spelling may not return as a keyword/alias). The remaining +9 morph integration vs. the prior count are #16's newly-localized codes (`e0502`/`e0505`/`e0202`/`e0401`), the `message_catalog_keys_are_real_checker_codes` guard, and the W0001 mixed-flavor lint tests.
- 2026-06-15 fuzz/security audit of the since-2026-06-14 changes: a deterministic stress harness over adversarial Tamil/keyword/ASCII input found that reskinning a numeric literal directly abutting a Tamil keyword/identifier (`42தொகுதி`) glued it into an unlexable lexeme — fixed by a boundary-space guard in `reskin`; and that `--names-map`accepted any`NameMap.version`— fixed by a version check in`load_name_map`. +1 translate integration (boundary guard regression), +1 config integration (unknown-version rejected). No overflow/unsafe/crash found. A `translate_roundtrip`cargo-fuzz target was added to close the coverage gap, CI-only, outside this count.
- 2026-06-15`mimz.toml`config + name-map auto-discovery: a new`config`module reads per-project flag defaults from`mimz.toml`(discovered by walking up from the input file; precedence CLI › config › default), and reverse`translate`auto-loads the`<input>.names.json` sidecar with no flag (`--no-names-map` opts out). +4 lib unit (`config`: parse, defaults, unknown-key reject, walk-up discovery), +4 config integration (auto-restore, --no-names-map, config precedence, malformed-config error).
- 2026-06-15 reversible romanization: `--romanize-names`now writes a per-file sidecar`<out>.names.json` (`NameMap`, romanized→Tamil) beside `-o`, and `mimz translate --names-map <file>`restores the exact Tamil names — so`Tamil → Latin → Tamil`is lossless. New`romanize_with_map`/`restore_with_map`share a factored`reskin` helper. +3 lib unit (`translate`: inverse map, restore inverts romanize, NameMap serde), +2 translate integration (lib round-trip via map, CLI forward+reverse).
- 2026-06-15 pure-Tamil showcase + opt-in `translate --romanize-names`: a new `examples/tamil-pure/`folder holds fully-Tamil programs — Tamil keywords AND identifiers — exempt from the four-flavor byte-identity rule (R9) and instead proven equivalent to their English counterparts by canonical identifier renaming.`mimz translate --romanize-names`reuses the emitter's`romanize` to rewrite Tamil identifiers to Latin (opt-in, one-way; lossless default unchanged). +2 lib unit (`translate`), +2 example integration (pure-Tamil golden + equivalence), +1 Icarus (pure-Tamil testbenches), +3 translate integration.
- 2026-06-15 mixed-flavor lint: a non-fatal warning **W0001** fires when a file mixes Tamil keywords with English/Tanglish — `Diag`gained a`Severity`(Error/Warning),`check`/`compile`/`eval`print it and still succeed, and the LSP shows it as a WARNING. +2`morph`lib unit, +1 LSP unit, +3`morph`integration.
- 2026-06-15 robustness follow-up to the 2026-06-14 batch audit: +9 lib unit — 2`morph`(tie-break + empty-stem inflection), 5 checker (two-literal`min`E0407,`nand`of a bare`bit`, nested `abs(min)`/`min(abs)`, `abs`at the width boundary), 1 parser (a long flat binary chain parses without tripping the E1113 depth guard), 1 emitter (a built-in lowers parenthesized inside a larger expression) — and +2`fmt` integration (`-o`onto the input path round-trips via the new atomic write; an unknown`--to`is a clean error). A`pretty_roundtrip`cargo-fuzz target was added (CI-only, outside this count).
- A QA pass for the new built-ins added the`bitops`example in all four flavors — golden + a self-checking Icarus testbench incl. the abs(MIN) width-growth case — plus edge tests: parser arity E1110, checker literal-adapt + abs-of-literal, fmt keyword-free/non-lexing, and`compile --lang`localization.
- Arithmetic built-ins`min`/`max`/`abs`/`nand`/`nor`/`xnor`added 6 checker unit tests + 1`eval`integration test.
- Phase 1.8 error-language plumbing added 8`morph`lib unit tests + 7`tests/morph.rs`integration tests for selection, inflection, and the additive English-fallback path.
- 2026-06-14, after merging the security-hardening and Phase 1.8 grammar branches: the security audit added 2 parser unit tests + 3`eval`integration tests for overflow/recursion guards; the Phase 1.8 thamizh-order flips — conditional / if-expression / match — added 10 grammar integration tests incl. the profile-boundary and depth-guard regressions. Then`mimz translate --order`(the`pretty`AST printer) added 4 translate integration tests + 1 grammar test for the Tamil thamizh-order traffic light.
- The error-fixture tests are data-driven over ~70 broken`.mimz`fixtures; one locks`ALL_CHECKER_CODES`— now`pub`in`src/diag.rs`— to the 11-checker.md catalog, one locks the`--json`wire format.
- The 2026-06-13 quick-wins block added the tooling tests below:`explain`(+3),`translate`(+3 unit, +3 integration),`sim::comb`(+7 unit, +6`eval` integration).

## Unit: keyword table (`src/lexer/keywords.rs`, 5 tests)

| Test                                        | Locks in                                                                                                                                                                             | If it fails…                                               |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| `all_three_flavors_resolve_to_same_keyword` | EN/Tanglish/Tamil spellings → one `Kw` token                                                                                                                                         | `lang/keywords.toml` edit broke a mapping                  |
| `flavors_are_recorded`                      | the lexer remembers which column a spelling came from                                                                                                                                | flavor tracking broke (P1.8 depends on it)                 |
| `include_is_an_alias_for_import`            | `include` lexes to the exact same token as `import`                                                                                                                                  | the alias mechanism or table entry broke                   |
| `fall_is_an_active_keyword`                 | `fall` lexes as KW_FALL in all three flavors (A3 promoted it from reserved)                                                                                                          | someone changed `fall`'s keyword status without a decision |
| `future_keywords_are_reserved_not_usable`   | every reserved future keyword (`fn`/`function`, the v0.3 backlog `secret`…`await`, section-8 `fixed`/`requires`/`ensures`, and `extern`) stays reserved and is not an active keyword | a future keyword was claimed without a decision (R11)      |
| `canonical_spellings_lists_every_keyword_in_a_flavor` | `canonical_spellings(flavor)` returns one spelling per `Kw` (31) in the asked column — the LSP's flavor-matched keyword completion list                          | the reverse-lookup table or completion keyword source broke |

Note: the table's structural rules (disjoint columns, known keys, valid
TOML) need no dedicated test — the `LazyLock` panics at startup, so
**every** test fails if the table is broken. That's by design.

## Unit: lexer (`src/lexer/tests.rs`, 11 tests)

| Test                                           | Locks in                                                                                                  |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `lexes_mixed_flavors`                          | mixing three flavors in ONE line works — the migration path                                               |
| `tamil_identifiers_work`                       | Tamil-script identifiers lex as identifiers (XID rules)                                                   |
| `numbers`                                      | decimal / `0b` / `0x` parse, `_` separators, correct values                                               |
| `wrapping_operators`                           | `+%` / `-%` are single tokens                                                                             |
| `larrow_vs_comparison`                         | `<-` vs `<=` vs `<<` disambiguation — longest match                                                       |
| `newline_continuation_after_operator`          | the Go-style newline policy, both directions (kept AND dropped)                                           |
| `division_is_rejected_with_teaching_error`     | `/` errors AND the help text teaches the alternative                                                      |
| `a_reserved_word_is_an_error`                  | a reserved word (`sync`) is a clean E1005, not a silent identifier (was `mem`)                            |
| `mem_is_an_active_keyword`                     | `mem`/`ninaivagam`/`நினைவகம்` lex as KW_MEM in all three flavors (A4 promoted it from reserved)           |
| `async_is_an_active_keyword`                   | `async`/`otthisaivatra`/`ஒத்திசைவற்ற` lex as KW_ASYNC in all three flavors (A5 promoted it from reserved) |
| `dont_care_binary_literal_lexes_to_masked_int` | `0b1??` lexes to `MaskedInt` (value/mask/width); plain `0b101` stays `Int` (A2)                           |

## Unit: parser (`src/parser/tests.rs`, 37 tests)

| Test                                                               | Locks in                                                                                |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| `parses_counter`                                                   | the canonical example parses; module has the expected 6 items                           |
| `parses_tanglish_counter_to_same_shape`                            | Tanglish source → structurally identical AST (the thesis, AST level)                    |
| `thamizh_order_on_block_parses_to_the_same_shape`                  | `syntax thamizh` + `yetram(clk) pothu { }` → the same module (spec/04)                  |
| `on_fall_parses_with_the_fall_edge`                                | `on fall(clk)` parses to `OnBlock` with `Edge::Fall` (A3)                               |
| `thamizh_order_on_fall_parses_to_the_fall_edge`                    | `irakkam(clk) pothu { }` → the same fall block (thamizh order) (A3)                     |
| `english_syntax_thamizh_directive_also_selects_the_profile`        | flavor and word-order profile are orthogonal (`syntax thamizh` in English)              |
| `unknown_syntax_profile_is_e1112`                                  | `syntax wibble` → E1112, not silently ignored                                           |
| `flipped_on_block_needs_the_directive`                             | a leading `rise(...)` is a parse error without the directive (gated flip)               |
| `thamizh_order_test_header_parses_to_the_same_shape`               | `M(args) kaaga "…" sodhanai { }` → the SAME `TestDecl` as the code-order header (B7)    |
| `thamizh_test_header_with_no_params_parses`                        | the flipped test header with no params (`Counter kaaga "…" sodhanai`) parses            |
| `the_test_header_flip_needs_the_directive`                         | a leading identifier test header without `syntax thamizh` is E1102 (gated flip)         |
| `a_long_flat_binary_chain_parses_without_tripping_the_depth_guard` | a 5000-term `a + a + …` chain parses — LENGTH is unbounded, distinct from nesting DEPTH |
| `rust_precedence_defuses_the_c_trap`                               | `x & 1 == 0` parses as `(x & 1) == 0` — **never** change this                           |
| `monotonic_chained_comparison_desugars_to_and`                     | `0 <= x <= 7` desugars to `(0<=x) && (x<=7)` — the safe Python form (8.9)               |
| `mixed_direction_chain_is_an_error`                                | `a < b > c` stays E1109 (the confusing form)                                            |
| `equality_cannot_be_chained`                                       | `a == b == c` stays E1109                                                               |
| `wire_if_without_else_teaches_about_latches`                       | mandatory `else` on if-expressions + the latch help text                                |
| `reg_without_reset_value_is_an_error`                              | mandatory reg reset (safety rule)                                                       |
| `assign_arrow_confusion_teaches`                                   | `=` inside `on` → help text pointing to `<-`                                            |
| `parses_repeat_and_const`                                          | `repeat i: 0..8` and file-level `const` parse                                           |
| `parses_test_block`                                                | `test "..." for M(...) { tick/expect }` parses                                          |
| `every_parse_error_carries_a_code`                                 | the E11xx retrofit, locked from outside: no parse error is codeless                     |
| `builtin_with_wrong_arity_is_e1110`                                | a built-in called with the wrong argument count (e.g. `min(a)`) is E1110                |
| `stray_top_level_brace_does_not_hang`                              | a stray top-level `}` errors and terminates — `file()` cannot spin (OOM)                |
| `deeply_nested_expression_errors_not_overflows`                    | `(((…)))` past the depth cap → clean E1113, not a stack overflow (SEC-1)                |
| `deeply_nested_unary_errors_not_overflows`                         | `!!!!…x` prefix chain → E1113 via the `unary` guard, not a crash                        |
| `replication_parses_to_replicate`                                  | `{2{a}}` parses as `Replicate` (count + inner parts), not concatenation (A1)            |
| `braces_without_an_inner_group_stay_concat`                        | `{a, a}` still parses as `Concat` — the replication path is no regression               |
| `dont_care_pattern_parses_to_intmask`                              | `0b1??` in a match arm parses as `Pattern::IntMask` (value/mask/width) (A2)             |
| `mem_declaration_parses_to_a_mem_item`                             | `mem m: bits[8][4] = 0` parses to `ModuleItem::Mem` (name/ty/depth/init) (A4)           |
| `a_mem_without_an_init_value_is_e1104`                             | a `mem` missing its `= init` is E1104 (no uninitialized state), like a reg (A4)         |
| `async_reset_parses_with_the_async_flag`                           | `async reset rst` sets `Reset.is_async` (A5)                                            |
| `a_plain_reset_is_synchronous`                                     | a bare `reset rst` leaves `is_async` false — sync is the default (A5)                   |
| `parse_recover_keeps_good_items_around_a_bad_one`                  | `parse_recover` leaves one `ModuleItem::Error` for a bad line; both ports survive       |
| `parse_recover_top_level_error_keeps_following_module`             | file-level garbage becomes `TopItem::Error`; the next module still parses               |
| `parse_recover_seq_and_test_blocks_emit_error_nodes`               | a bad stmt in `on`/`test` yields `Seq`/`TestStmt::Error`; good stmts survive            |
| `strict_parse_still_errs_on_bad_input`                             | the strict `parse` contract is unchanged — any error discards the tree                  |

The error-path tests assert on message/help **substrings** (loose, so
wording can be polished) AND on the stable E-code (tight — the
contract). Lexer error tests do the same with E10xx.

## Unit: checker (`src/checker/tests.rs`, 112 tests)

One test per error code plus clean-pass cases — the codes are the
stable contract, so each test asserts the CODE and a message substring
(loose on wording). The full catalog with meanings lives in
[`11-checker.md`](11-checker.md); the test names map one-to-one
(`unknown_name_is_e0101_with_teaching_help`, `assignment_width_mismatch_is_e0401`, …).
The width slice (E0401–E0410) added 26: error paths for every code
(several codes get two angles, e.g. `extend`-narrowing AND
`trunc`-widening for E0407) plus six clean passes. The driver slice
(E0501–E0505) added 16: every code's error paths (both halves where a
code covers two mistakes, e.g. zero AND multiple `on` blocks for E0503)
plus four clean passes guarding against false positives. The completion
slice (E0302/E0601/E0602/E0701, 2026-06-12) added 20: exhaustiveness
over enums/`bit`/`bits[N]` incl. the v0.2.3 rulings as clean passes,
instantiation completeness both ways (missing AND duplicate), and the
clock-domain matrix (independent domains clean, direct read,
through-a-wire, domain-mixing wire, unused-second-clock clean). A few
deserve a note:

| Test                                                                  | Locks in                                                                               |
| --------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `clean_module_passes` / `const_arithmetic_and_repeat_bounds_evaluate` | clean code produces ZERO diagnostics — the checker must never cry wolf                 |
| `duplicate_module_across_files_is_e0001_in_the_right_file`            | checker diagnostics carry the file index (multi-file rendering contract)               |
| `plus_into_same_width_target_teaches_wrap_in_e0401`                   | the dropped-carry moment teaches `+%` — the spec/02 section 1.2 promise, executable    |
| `defaultless_param_module_is_checked_per_instantiation`               | a module with no param defaults is checked under each instantiation's concrete binding |
| `replication_width_is_count_times_inner`                              | `{2{bits[4]}}` is `bits[8]`, `{3{bits[4]}}` is `bits[12]` (A1)                         |
| `replication_width_mismatch_is_e0401`                                 | `{2{a}}` (bits[8]) into a `bits[4]` is the usual assignment width error                |
| `a_non_constant_replication_count_is_e0201`                           | `{n{a}}` with a signal count is "not a compile-time constant" (reused code)            |
| `a_zero_replication_count_is_e0410`                                   | `{0{a}}` has zero width — reuses the "not a valid width" code                          |
| `dont_care_pattern_must_match_the_scrutinee_width`                    | `0b1??` is fine on `bits[3]`, a width error (E0409) on `bits[4]` (A2)                  |
| `a_dont_care_match_still_needs_a_wildcard`                            | masked patterns earn no coverage — `0b1??`+`0b0??` without `_` is E0601 (A2)           |
| `a_dont_care_pattern_on_an_enum_is_e0409`                             | a masked pattern on an enum scrutinee is rejected (match variants by name) (A2)        |
| `repeat_index_out_of_range_at_the_last_iteration_is_e0406`            | `repeat` bodies are width-checked per iteration value, not just once                   |
| `extend_of_a_bit_into_bitwise_passes`                                 | the fixed shift-register shape — explicit `extend` where widths differ                 |
| `disjoint_per_bit_drives_via_repeat_pass`                             | the Chaser idiom: eight `led[i] = ...` drives are eight drivers for eight bits — legal |
| `repeat_instance_array_ripple_carry_is_not_a_cycle`                   | per-index instance-output nodes: `fa[1] -> fa[0]` is a chain, not a loop               |
| `a_cycle_through_instances_is_e0504`                                  | combinational loops THROUGH child modules are caught via the comb summaries            |
| `feedback_through_a_register_is_not_a_cycle`                          | a reg breaks the loop — the normal shape of hardware never false-positives             |
| `enum_match_covering_every_variant_needs_no_wildcard`                 | the v0.2.3 ruling, executable: full coverage IS exhaustive, no `_` ceremony            |
| `wildcard_after_full_enum_coverage_is_allowed`                        | the defensive `_` (bit-flip recovery) is never flagged unreachable                     |
| `clock_and_reset_ports_may_be_omitted`                                | E0302 exempts clock/reset — implicit-by-name stays the emitter's contract              |
| `same_domain_logic_under_two_declared_clocks_passes`                  | E0701 colors by USE, not by declaration count — an unused clock changes nothing        |
| `register_file_passes`                                                | a `mem` with a clocked indexed write + combinational indexed read checks clean (A4)    |
| `a_non_constant_memory_depth_is_e0201`                                | a memory `DEPTH` that is not a compile-time constant is E0201 (A4)                     |
| `a_zero_memory_depth_is_e0410`                                        | a memory `DEPTH` of 0 is E0410 — a memory needs at least one cell (A4)                 |
| `a_memory_init_that_overflows_the_element_is_e0405`                   | a `mem` init value too wide for the element type is E0405 (A4)                         |
| `a_constant_address_past_the_depth_is_e0406`                          | a compile-time address `≥ DEPTH` is E0406 (out of range) (A4)                          |
| `a_memory_inside_repeat_is_e0303`                                     | declaring a `mem` inside `repeat` is E0303 (declare once, outside) (A4)                |

## Unit: transliteration (`src/emit_verilog/translit.rs`, 5 tests)

| Test                                      | Locks in                                                              |
| ----------------------------------------- | --------------------------------------------------------------------- |
| `pure_tamil_words_romanize_readably`      | விளக்கு → `villakku`, நிலை → `nilai` — the readable-output promise    |
| `ascii_and_mixed_names_keep_their_ascii`  | ASCII passes through untouched, even mixed into a Tamil name          |
| `non_tamil_unicode_falls_back_to_hex`     | other scripts → `_uXXXX`, never dropped                               |
| `results_always_start_like_an_identifier` | output is always a valid Verilog identifier start                     |
| `the_two_n_letters_romanize_identically`  | ந/ன → `n` is a DOCUMENTED collision; the suffix counter disambiguates |

## Unit: emitter (`src/emit_verilog/mod.rs`, 16 tests)

| Test                                                            | Locks in                                                                                                                                    |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `diags_carry_the_file_index`                                    | project-level diagnostics (duplicate module, emit errors) record WHICH file they point into, so multi-file errors render the right excerpt  |
| `repeat_unrolls_drives_with_folded_indices`                     | `repeat i: 0..4 { y[i] = … }` emits `assign y[0..3]`; the half-open range stops at 3                                                        |
| `on_fall_emits_negedge`                                         | `on fall(clk)` lowers to `always @(negedge clk)` (A3)                                                                                       |
| `async_reset_widens_the_sensitivity_list`                       | `async reset` lowers to `always @(posedge clk or posedge rst)` (A5)                                                                         |
| `a_sync_reset_stays_clock_only`                                 | a plain `reset` keeps `always @(posedge clk)` — no sensitivity widening (A5)                                                                |
| `repeat_var_folds_in_index_arithmetic`                          | `y[i + 1]` folds to `y[1]`/`y[3]` — index arithmetic over the loop var collapses to a literal                                               |
| `empty_and_reversed_ranges_emit_nothing`                        | `0..0` and `4..0` generate no hardware (no crash, no partial output)                                                                        |
| `repeat_over_budget_errors_cleanly`                             | a range past `REPEAT_BUDGET` (4096) is a clean error, not a runaway unroll                                                                  |
| `nested_repeat_folds_both_variables`                            | nested loops bind both `i` and `j` per iteration; `y[1] = 1` proves the inner+outer fold                                                    |
| `repeat_instance_array_gets_flat_names`                         | `let u[i] = …` → `u__<i>` with outputs `u__<i>_<port>`; `u[i].o` reads back the same flat wire                                              |
| `module_const_folds_in_widths_and_emits_no_hardware`            | a `const` folds to a literal in port widths and bounds, and declares no Verilog of its own                                                  |
| `child_consts_fold_into_parent_auto_wires`                      | instantiating a const-widthed module folds the CHILD's const into the auto-wire (regression: `wire [(W)-1:0]` leaked and iverilog rejected) |
| `parent_const_never_substitutes_into_child_widths`              | same const NAME in parent and child: the child's value sizes the wire — never the parent's (silently wrong hardware otherwise)              |
| `tamil_identifiers_emit_as_romanized_verilog`                   | the transliterated pipeline end to end: module/ports/regs/always all use the SAME romanization; no non-ASCII outside the banner comment     |
| `colliding_romanizations_get_suffixes_and_ascii_names_are_safe` | ந/ன clash + an existing ASCII `nii`: user names are never stolen; clashes get `_2`, `_3` deterministically                                  |

## Unit: testbench emitter (`src/emit_verilog/testbench.rs`, 4 tests)

| Test                                            | Locks in                                                                                                                                                         |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sanitize_verilog_ident_replaces_invalid_chars` | spaces/symbols/leading digits/empty string all sanitize to a valid Verilog identifier                                                                            |
| `test_env_falls_back_to_module_param_defaults`  | `--emit-testbench` resolves a width expression for a module parameter the test never overrides, from its `default` (BUG-3)                                       |
| `test_env_chains_earlier_args`                  | a test's later `(NAME: expr, …)` arg may reference an earlier one in the same list, e.g. `DOUBLE: WIDTH * 2`                                                     |
| `colliding_sanitized_test_names_are_rejected`   | two tests whose names sanitize to the same Verilog module id (`"edge case"`/`"edge_case"` -> `edge_case_tb`) error instead of emitting duplicate modules (BUG-4) |

## Integration (`tests/examples.rs`, 13 tests — run the real binary)

`examples/` holds four flavor folders — `english/`, `tanglish/`, `tamil/`,
`mixed/` — each with the SAME 23 base designs + 1 `lib/` helper + 5 `std/`
modules (29 `.mimz` files total; identical identifiers, only keywords differ;
`lib/` and `std/` subfolders hold dotted-import targets and the standard-library
modules). The base-example list lives in the
`BASE_EXAMPLES` const in the test file. (`bitops` — the arithmetic / reduction
built-ins — and `datapath` — `*`/`*%`, `>>`, concat, slice, `trunc` — were added
2026-06-14; the five `std/` modules — `debouncer`, `seg7`, `pwm`, `fifo`,
`uart_tx` — over 2026-06-13…23. The FIFO originally used an explicit `DEPTH`
parameter to work around BUG-6; after the fix it was reverted to `1 << AW`.)

A fifth folder, `examples/tamil-pure/`, holds the **pure-Tamil showcase** —
fully-Tamil programs (Tamil keywords AND identifiers; the `PURE_TAMIL` const
pairs each with the English base example it mirrors). Being language-pure, they
are NOT byte-identical to any other flavor, so they sit OUTSIDE the four-flavor
identity rule (R9) and are validated by equivalence-to-counterpart + their own
goldens (`tests/golden/tamil_pure_*.v`) + their own testbenches.

| Test                                                         | Locks in                                                                                                                                                                                                                                                                                                                     |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_example_checks_clean`                                 | every `.mimz` under `examples/` (recursive) passes `mimz check` — which now runs the CHECKER over the file and its imports, so this is also a zero-false-positives test for every checker rule. At least 4 × 28 base files (plus `lib/` helpers, the `std/` modules, and the pure-Tamil showcase) — RULES R6 made executable |
| `every_example_compiles`                                     | every example **compiles to Verilog**, including the `lib/` helpers. A new example that doesn't compile fails CI by name                                                                                                                                                                                                     |
| `all_four_flavors_compile_to_identical_verilog`              | each base example → **byte-identical** Verilog from all four flavors. The project's thesis. Never break it                                                                                                                                                                                                                   |
| `counter_compiles_to_verilog`                                | end-to-end compile; asserts the parameter, the always-block, the **generated reset**, the assign                                                                                                                                                                                                                             |
| `alu_with_import_compiles`                                   | `import` resolution end-to-end; instances with params; auto-wired child outputs (`add_sum`)                                                                                                                                                                                                                                  |
| `include_alias_compiles_with_dotted_path`                    | `include lib.full_adder` works through the whole pipeline — the alias AND dotted-path resolution, in one example (`english/chained.mimz`)                                                                                                                                                                                    |
| `ripple_adder_unrolls_repeat`                                | `repeat` end-to-end: four `FullAdder fa__0..3` with the carry chained, folded indices, `const WIDTH` folded into widths — compile-time generation proven through the real binary                                                                                                                                             |
| `traffic_light_fsm_compiles`                                 | enums → localparams (`STATE_RED` …)                                                                                                                                                                                                                                                                                          |
| `emitted_verilog_matches_the_goldens`                        | every base example's FULL output equals `tests/golden/<base>.v` byte for byte (banner stripped). On an INTENDED emitter change: `MIMZ_UPDATE_GOLDENS=1 cargo test --test examples`, then review the golden diff like code. Failure names the first differing line                                                            |
| `emitted_testbench_matches_the_goldens`                      | every base example with inline `test` blocks generates a `_tb.v` byte-identical to `tests/golden/<base>_tb.v` (banner stripped); `MIMZ_UPDATE_GOLDENS=1` regenerates — pins the auto-generated `--emit-testbench` output                                                                                                     |
| `emit_testbench_without_test_blocks_notes_and_writes_only_v` | `mimz compile --emit-testbench` on a source with NO `test` blocks still succeeds, writes only the `.v` (no stray `_tb.v`), and prints a no-effect note on stderr — the flag never silently produces nothing                                                                                                                  |
| `pure_tamil_examples_match_goldens`                          | each `examples/tamil-pure/<x>.mimz` output equals `tests/golden/tamil_pure_<x>.v` (banner stripped) — pins the transliterated Verilog so a romanization regression can't slip through                                                                                                                                        |
| `pure_tamil_examples_are_equivalent_to_their_counterparts`   | each pure-Tamil example is the SAME circuit as its English twin, proven by `canonicalize_verilog` (alpha-equivalence: identifiers renamed to `id<N>` by first appearance). Equal canonical forms ⇒ same hardware, just named in Tamil                                                                                        |

## Icarus differential (`tests/icarus.rs`, 5 tests — run a REAL Verilog tool)

The independent judge: our substring asserts check OUR expectations of
the output; these check a real tool's. **Skips with a printed note when
`iverilog` is not installed** (probe order: `MIMZ_IVERILOG` env →
PATH → the Windows installer default `C:\iverilog\bin`); in CI
`REQUIRE_IVERILOG=1` makes a missing install a hard failure, so CI can
never skip silently. Local install: the Windows installer
(bleyer.org/icarus) or `apt-get install iverilog`.

| Test                                        | Locks in                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_emitted_verilog_passes_iverilog`     | all 72 examples' emitted `.v` pass `iverilog -t null` — syntax AND elaboration, by Icarus's judgment (incl. the transliterated Tamil-identifier `vilakku`, the pure-Tamil showcase, and `wire signed` `signed_math`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `every_emitted_testbench_passes_iverilog`   | every base example's auto-generated `_tb.v` (from `--emit-testbench`) passes `iverilog -t null` — the generated testbenches are themselves valid, elaborable Verilog by Icarus's judgment, not just our goldens                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `self_checking_testbenches_pass`            | one hand-written TB per base example (`tests/icarus/*_tb.v`, counts vary — grew from 16 with stdlib and tamil-pure additions) encodes Min-Mozhi's documented semantics (`+%` wraps, sync reset, non-blocking `<-`, FSM timing, SIGNED extension/comparison, `bitops` min/max/abs(MIN)/nand/nor/xnor, `datapath` lossless `*` vs wrapping `*%`/`>>`/concat/slice/`trunc`) and must print PASS under `vvp` — the differential                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `self_checking_pure_tamil_testbenches_pass` | the four pure-Tamil showcase circuits (`kanakki`/`cimitti`/`oppidi`/`thervi`), driven through their **romanized** ports (clk=`katikai`, rst=`miill`, …) — proves the transliterated Verilog SIMULATES, not just elaborates. Shares the `run_self_checking` helper with the English layer                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `our_simulator_matches_icarus_bit_for_bit`  | **Layer 3 (B8 + C1–C4):** three views must agree bit-for-bit per step — our kernel (in-process), the VCD waveform our writer emits, and Icarus on the emitted Verilog under the same stimulus. Auto-routes per design: **clocked** (counter, shift register, edge detector, blinker @ `LIMIT=3`) and **combinational** over generated input vectors (adder, comparator, mux4, datapath, window, full_adder + SIGNED `bitops`/`signed_math`) — 12 ASCII-named english examples — plus the 6 pure-Tamil showcases (kanakki/cimitti/oppidi/thervi/kuutti/saalaivilakku, driven through romanized port names) and the full-parity additions: **alu** (cross-file instance, C2), **chained** (chained instances, C2), **ripple_adder** (`repeat`, C3), **traffic_light** (enum FSM, C4), and **vilakku** (Tamil identifiers). **21 examples** in all — the entire single-file corpus the emitter compiles. Compared via Verilog `%b` (binary ⇒ signedness-agnostic). Where Layer 2 checks Icarus against hand-written asserts, this pits our simulator (engine AND waveform) directly against Icarus |

House rule for the testbenches: each prints `PASS` exactly once or
`FAIL: reason` and stops — the Rust side asserts on those markers, so a
broken TB fails loudly, never silently. The Blinker TB overrides the
`LIMIT` parameter (`#(.LIMIT(3))`) instead of simulating 50M cycles.

## Error fixtures (`tests/errors.rs`, 4 tests — run the real binary on broken code)

End-to-end **failure** validation, the mirror of the checker unit tests: those
prove the checker _function_ rejects bad code; these prove the _CLI_ surfaces it.
`tests/fixtures/errors/*.mimz` holds ~72 intentionally-broken files (kept OUT of
`examples/`, which is asserted valid), each declaring its expected code in a
`// expect: Exxxx` header. Source bodies are lifted from `src/checker/tests.rs`.

| Test                                           | Locks in                                                                                                                                                             |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_error_fixture_reports_its_code`         | each fixture, run through `mimz check`, exits non-zero AND prints `error[<code>]` to stderr — the rendered code is the stable user-facing contract, checked for real |
| `error_corpus_covers_every_checker_code`       | completeness guard: every code in `ALL_CHECKER_CODES` (the 36 stable checker codes) has at least one fixture — a new E-code can't ship without an end-to-end fixture |
| `checker_code_list_matches_the_catalog`        | `ALL_CHECKER_CODES` must equal the 11-checker.md catalog table (reserved rows exempt) — the corpus, the docs, and the code can't drift apart                         |
| `json_flag_emits_machine_readable_diagnostics` | the `--json` wire format (docs/code/06): one JSON array on stdout with code/path/line/help; lexer errors included; `[]` + exit 0 on success                          |

`every_error_fixture_reports_its_code` also asserts a `help:` line per
fixture — the teaching contract, proven at the CLI surface.

Coverage is **every distinct edge case**, not one per code: E0302 missing-input
AND duplicate-conn; E0407 extend-narrowing AND `-` on bits; E0303 all eight
forbidden declaration kinds; E0601 enum/`bits[N]`/`bit`; E0701's three crossings;
etc. The assertion is "stderr _contains_ the code", tolerant of a fixture that
incidentally trips a second rule. Convention + how-to: `tests/fixtures/errors/README.md`.

## Docs-sync (`tests/docs_sync.rs`, 4 tests)

The mechanical staleness guard for `docs/code/` — these verify the
structural facts the docs state, so doc drift fails CI. When one fails,
**fix the named doc page, don't weaken the test.**

| Test                                                | Locks in                                                                                             |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crate_map_lists_every_module`                      | both crate-map copies (`src/main.rs` `//!` table, `docs/code/README.md`) name every top-level module |
| `module_pages_list_every_source_file`               | each module page's file-layout table lists every `.rs` file actually in that `src/` directory        |
| `every_module_is_documented_somewhere_in_docs_code` | a new pipeline stage (e.g. `src/checker/`) cannot land without a docs mention                        |
| `code_docs_have_a_sync_stamp`                       | the "Last synced" tripwire line survives                                                             |

## Grammar-sync (`tests/grammar_sync.rs`, 6 tests)

Same philosophy as docs-sync, for the keyword data: the keyword table is
data, so the TextMate grammar and the human-readable spec mirror can silently
drift. Whole-member matching throughout, because `in` is a substring of
`include` — a plain `contains` would pass vacuously. When one fails: fix the
grammar / the spec, don't weaken the test.

| Test                                           | Locks in                                                                                                                                              |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_keyword_spelling_is_in_the_grammar`     | every spelling (canonical + aliases) appears as a whole alternation member in the VS Code grammar                                                     |
| `every_reserved_word_is_marked_invalid`        | every reserved word appears in the grammar's `invalid.illegal` rule                                                                                   |
| `spec_03_keyword_table_matches_keywords_toml`  | every spelling appears in `spec/03` as a backtick word — the spec mirror can't drift after the v1 lock                                                |
| `spec_04_uses_no_superseded_keyword_spellings` | `spec/04`'s worked examples contain none of the 14 superseded v1 spellings (whole-word, Tamil-aware)                                                  |
| `keywords_toml_has_no_superseded_spelling`     | a superseded v1 spelling may never return in `lang/keywords.toml` as a canonical spelling or any alias — guards the reintroduction risk at the source |
| `grammar_and_extension_manifest_agree`         | `package.json` registers `.mimz` and its scope name matches the grammar                                                                               |

## Editor analysis (`src/analysis.rs`, 7 lib unit tests)

The pure, async-free symbol index and resolution behind the LSP's hover /
go-to-definition / completion (the `src/lsp.rs` handlers are a thin adapter).

| Test                                                  | Locks in                                                                                                          |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `index_collects_each_definition_kind`                 | `build_index` emits a `Symbol` for every def kind (module, param, port, clock, reg, const, enum + variant, inst) with the right `SymKind` + hover render |
| `resolve_at_use_returns_definition`                   | a use site resolves to its **declaration** span, not the use                                                      |
| `resolve_at_works_on_partial_tree`                    | `parse_recover` `Error` node between good ports — names around it still resolve                                    |
| `resolve_at_inside_test_block`                        | inside `test "…" for M { … }`: the module-under-test name + driven inputs + `expect` signals resolve to M's ports (cross-file via `same_module_any_file`) |
| `resolve_at_cross_file_instance`                      | an instantiated imported module name resolves into the imported file (`file_idx` differs)                          |
| `completions_include_scope_idents_and_majority_keywords` | in-scope module members + majority-flavor keywords offered, with the right `CandKind`                           |
| `completions_exclude_other_flavor_keywords`           | a Tamil-flavored file offers Tamil keywords, never the English spellings (no cross-flavor leak)                    |

## LSP (`src/lsp.rs` unit + `tests/lsp.rs` smoke, 8 tests)

| Test                                                        | Locks in                                                                                                                                     |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `positions_are_utf16_lines_and_columns`                     | byte span → LSP Position math (0-based lines)                                                                                                |
| `offset_inverts_position_utf16`                             | `offset` is the exact inverse of `position` (UTF-16 units, incl. a Tamil line) — the cursor→byte mapping the feature handlers depend on      |
| `tamil_text_counts_utf16_units_not_bytes`                   | LSP columns are UTF-16 code units — a Tamil identifier before the error must not skew the squiggle                                           |
| `analyze_reports_checker_errors_with_codes`                 | the in-memory pipeline (didOpen text, never on disk) produces coded checker diagnostics                                                      |
| `diagnostics_localize_to_the_chosen_flavor`                 | the LSP renders E0501 in Tamil (`y-க்கு` via `morph`) and English verbatim — same plumbing as `check`/`compile`                              |
| `uncovered_code_is_not_localized_in_lsp`                    | an uncovered code (E0401) is byte-identical across flavors in the LSP (the English-fallback invariant)                                       |
| `opening_a_broken_file_publishes_coded_diagnostics` (smoke) | the REAL binary over the real wire protocol: framed JSON-RPC initialize → didOpen → publishDiagnostics with code, source, help, and position |

## Benchmark harness (`src/bin/mimz-bench/`, 6 unit tests)

The harness itself (docs in [`12-benchmark.md`](12-benchmark.md))
re-measures what this suite asserts — rates and timings instead of
pass/fail — so its own logic is unit-tested here:

| Test                                       | Locks in                                                       |
| ------------------------------------------ | -------------------------------------------------------------- |
| `rate_percent_handles_zero_and_partial`    | rate math (0/0 reads as 100%, never NaN)                       |
| `expect_header_parses_only_the_convention` | the `// expect: Exxxx` fixture-header parse, same as errors.rs |
| `banner_strip_matches_the_golden_rule`     | banner stripping byte-matches the golden test's rule           |
| `median_is_the_middle_run`                 | timing aggregation (median, robust to one cold run)            |
| `report_renders_a_complete_page` (html)    | the HTML report renders whole: charts, tables, embedded JSON   |
| `failures_flip_the_verdict_and_are_listed` | a failing validation turns the verdict red and is named        |

The `criterion` micro-benchmark harness (`benches/compile.rs`, run with
`cargo bench`) carries **no `#[test]`s** — `criterion` benchmarks aren't
test functions, so it doesn't affect the count above. It's a separate
performance tool, not part of the assertion suite.

## Unit: explain (`src/explain.rs`, 3 tests)

The 8.1 long-form diagnostic catalog behind `mimz explain <CODE>`.

| Test                                       | Locks in                                                                                       |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| `every_checker_code_has_an_explanation`    | every `ALL_CHECKER_CODES` entry has long-form text — a new checker code can't ship without one |
| `table_is_sorted_unique_and_self_labelled` | the `EXPLANATIONS` table is ordered, duplicate-free, and each entry opens with its own code    |
| `lookup_is_case_insensitive_and_trims`     | `explain("e0501")` / `" E0501 "` resolve; unknown codes return `None`                          |

## Unit: translate (`src/translate.rs`, 8 tests)

The keyword-flavor reskin behind `mimz translate --to`, plus the opt-in
`--romanize-names` identifier rewrite (reuses the emitter's `romanize`) and the
reversible sidecar name-map (`romanize_with_map` / `restore_with_map`).

| Test                                                        | Locks in                                                                                      |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `parse_flavor_accepts_the_three_columns`                    | `english`/`tanglish`/`tamil` (case-insensitive) parse; junk → `None`                          |
| `reskins_keywords_keeps_everything_else`                    | keywords swap; comments, layout, identifiers, numbers stay verbatim                           |
| `translating_to_the_same_flavor_is_identity_for_canonical`  | canonical English → English is a no-op                                                        |
| `romanize_names_rewrites_tamil_identifiers_only_when_asked` | `--romanize-names` turns `கணக்கு` → `kannakku`; the default leaves the Tamil name             |
| `romanize_names_uniques_against_an_existing_ascii_name`     | a romanization clashing with an ASCII name gets `_2` — names never silently merge             |
| `romanize_with_map_returns_the_inverse_map`                 | the sidecar map is keyed by the Latin spelling → original Tamil (`kannakku` → `கணக்கு`)       |
| `restore_with_map_inverts_romanize`                         | `restore(romanize(src), map)` reproduces the canonical Tamil source — the round-trip identity |
| `name_map_json_round_trips`                                 | `NameMap` serializes and deserializes through `serde_json` unchanged                          |

## Integration: translate (`tests/translate.rs`, 13 tests — the four-flavor oracle + the `--order` pretty-printer + `--romanize-names` + the sidecar name-map)

The `examples/{english,tanglish,tamil}/` folders are byte-identical
keyword-swaps (R9), so they validate the reskin against committed truth. Four
cover `--order` (the `pretty` AST printer): it reformats and drops comments, so
its oracle is semantic (same Verilog) + idempotency, not bytes. The final three
cover `--romanize-names` over the pure-Tamil showcase (Tamil identifiers → Latin,
opt-in and one-way; the default stays lossless).

| Test                                                               | Locks in                                                                                                                                                                       |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `round_trip_to_every_flavor_is_byte_identical`                     | translate-and-back reproduces the canonical source byte-for-byte (lossless; anchored past alias normalize)                                                                     |
| `translating_english_matches_the_committed_flavor_token_for_token` | translating english `X` to flavor `T` lexes identically to the committed `T/X` (comments excluded)                                                                             |
| `every_keyword_token_is_in_the_target_flavor`                      | the reskin actually fires — English `module` is gone, Tamil `தொகுதி` present                                                                                                   |
| `pretty_print_preserves_verilog_across_flavor_and_order`           | every import-free example × flavor × order pretty-prints to byte-identical Verilog (meaning preserved)                                                                         |
| `pretty_print_is_idempotent`                                       | the pretty-printer is a stable canonical form (re-printing its own output is a fixed point), all examples                                                                      |
| `thamizh_order_emits_the_directive`                                | thamizh output starts with `syntax thamizh` / `இலக்கணம் தமிழ்`; code order emits none                                                                                          |
| `cli_translate_order_thamizh_compiles`                             | `--order thamizh --to tamil` on the traffic light yields compilable, same-Verilog Tamil SOV source                                                                             |
| `romanize_names_converts_tamil_identifiers_to_latin`               | `--romanize-names` rewrites Tamil identifiers to Latin in the CODE (comments keep the original); no Tamil-script char survives outside comments                                |
| `romanized_translation_compiles_to_the_same_verilog`               | romanizing then compiling a pure-Tamil file is byte-identical to compiling the original — the romanization matches the emitter's, so meaning is preserved                      |
| `pure_tamil_round_trips_losslessly`                                | the DEFAULT (no flag) still round-trips Tamil → English → Tamil byte-for-byte — the lossless contract holds for Tamil-named files too                                          |
| `romanized_round_trips_losslessly_via_the_name_map`                | romanize (capturing the `NameMap`) then `restore_with_map` reproduces the canonical Tamil source — the one-way romanization made lossless by the sidecar                       |
| `cli_romanize_then_restore_round_trips`                            | end-to-end through the binary: `--romanize-names -o` writes a parseable `<out>.names.json`; a reverse run with `--names-map` restores the exact Tamil source                   |
| `number_abutting_tamil_keeps_a_separator_when_reskinned`           | fuzz-audit regression: `42தொகுதி`/`42கணக்கி` (number + Tamil token, script change as the only separator) stays lexable + token-equivalent after reskin (guard inserts a space) |

## Unit: config (`src/config.rs`, 4 tests)

`mimz.toml` parsing + discovery (the precedence merge lives in `main.rs` and is
exercised by the integration tests below).

| Test                                      | Locks in                                                                                 |
| ----------------------------------------- | ---------------------------------------------------------------------------------------- |
| `empty_config_is_all_defaults`            | an empty/missing config is all `None` — pure built-in defaults                           |
| `parses_every_section`                    | `lang` + `[translate]` + `[fmt]` keys deserialize to the right fields                    |
| `unknown_key_is_rejected`                 | a typo'd key (`too`, `flavour`) errors via `deny_unknown_fields`, never silently dropped |
| `discover_walks_up_to_the_nearest_config` | discovery climbs from a nested file to the ancestor `mimz.toml`                          |

## Unit: version (`src/version.rs`, 3 tests)

The two version axes — compiler (crate) vs language edition — and the
`EDITION_HISTORY` source of truth (Workstream B).

| Test                                        | Locks in                                                                                             |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `current_is_the_last_history_row`           | `current()` is the tail of `EDITION_HISTORY`, which stays ordered oldest-first by (year, code)       |
| `keyword_set_version_matches_keywords_toml` | `KEYWORD_SET_VERSION` == `lang/keywords.toml`'s `version` == the current edition's `code` (no drift) |
| `version_block_shows_both_axes`             | `mimz --version` block has the variant on top + the compiler and edition (language) lines            |

## Integration: config (`tests/config.rs`, 5 tests — run the real binary)

The CLI merge (CLI › config › default) and name-map auto-discovery, end to end.

| Test                                             | Locks in                                                                                   |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------ |
| `auto_name_map_restores_without_a_flag`          | reverse translate auto-loads `<input>.names.json` and restores Tamil — no `--names-map`    |
| `no_names_map_keeps_latin_names`                 | `--no-names-map` opts out of auto-discovery; the romanized Latin decl stays                |
| `config_default_flavor_is_overridden_by_the_cli` | `[translate] to` supplies the default; an explicit `--to` overrides it                     |
| `malformed_config_is_a_clean_error`              | a broken `mimz.toml` fails with `invalid config`, not a panic                              |
| `name_map_with_unknown_version_is_rejected`      | a `--names-map` with an unknown `version` fails closed (`version 999`), never mis-restores |

## Integration: stdlib (`tests/stdlib.rs`, 11 tests — drive the lib in-process)

The importable `std.*` library: embedded resolution, trilingual alias routing,
the `[lib]` override, the `mimz eject std` core, and the regression that plain
file-relative imports still work. The 5 catalog-level unit tests
(`src/stdlib.rs`: namespace aliases, canonical-vs-twin routing, unknown-module,
the no-transitive-imports invariant) and the 3 config-level unit tests
(`src/config.rs`: `[lib]` parse, unknown-key reject, `resolve_with_path`) back
these.

| Test                                                         | Locks in                                                                                                                                                                      |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `embedded_std_import_resolves_without_filesystem`            | `import std.fifo` loads the compiler-embedded module (synthetic `std:fifo.mimz`) AND the entry stays `files[0]` (std appended last) — `sim`/`test` elaborate `files[0]`       |
| `tamil_twin_routes_to_twin_source`                           | `சேர்க்க நூலகம்.வரிசை` routes to the **pure-Tamil twin** source (`தொகுதி வரிசை`), not the English canonical                                                                   |
| `unknown_std_module_errors_with_available_list`              | `import std.nope` is **E1202** and the message lists the available stems (`fifo`, …)                                                                                          |
| `wrong_std_arity_errors`                                     | `import std.fifo.extra` (three segments) is rejected — a std import is exactly `std.<module>` (E1202)                                                                         |
| `plain_relative_import_still_works`                          | a non-std `import helper` still resolves file-relative — the std branch is no regression                                                                                      |
| `lib_std_override_wins_over_embedded`                        | `[lib] std = "<dir>"` makes `import std.fifo` load `<dir>/fifo.mimz` (a sentinel), not the embedded `Fifo`                                                                    |
| `lib_std_override_filename_matches_eject_for_twin_spellings` | with an ejected Tamil dir, both `import std.வரிசை` and `import std.varisai` resolve to `varisai.mimz` — the override filename keys on the resolved variant, not the raw alias |
| `eject_writes_english_modules`                               | `eject_to(dir, false, false)` writes all 5 English canonical modules; `fifo.mimz` contains `module Fifo`                                                                      |
| `eject_tamil_writes_twins`                                   | `eject_to(dir, true, false)` writes the pure-Tamil twins (`varisai.mimz` contains `தொகுதி வரிசை`)                                                                             |
| `eject_refuses_overwrite_without_force`                      | a second eject over existing files fails; `force = true` overwrites                                                                                                           |
| `eject_is_all_or_nothing_on_partial_conflict`                | one pre-existing target aborts the whole eject before any other file is written — no half-vendored directory                                                                  |

## Unit: morph (`src/morph.rs`, 14 tests)

Error-language selection + Tamil case-suffix inflection (Phase 1.8, spec/04 section 5),
the W0001 mixed-flavor lint, and the structured-arg / English-fallback guards.

| Test                                                      | Locks in                                                                                                                                     |
| --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `majority_picks_the_dominant_keyword_flavor`              | all-English vs all-Tamil keyword files resolve to English / Tamil                                                                            |
| `majority_falls_back_to_english_with_no_keywords`         | a keyword-free token stream defaults to English                                                                                              |
| `majority_breaks_ties_toward_the_earliest_keyword_column` | a flavor tie resolves deterministically to the earliest keyword column                                                                       |
| `effective_lang_override_beats_majority`                  | `--lang` wins over the file majority; absence uses the majority                                                                              |
| `parse_lang_matches_translate_flavor`                     | `--lang` parsing reuses `translate::parse_flavor` (spellings never drift)                                                                    |
| `inflect_attaches_each_case_suffix`                       | each case attaches its spec suffix; Latin stems hyphenate, Tamil joins, English none                                                         |
| `inflect_of_an_empty_stem_is_empty_not_a_bare_suffix`     | inflecting an empty stem yields empty — never a dangling case suffix                                                                         |
| `suffix_table_has_every_case`                             | `lang/case_suffixes.toml` parses and defines all four cases (startup validation)                                                             |
| `localized_is_none_for_uncovered_codes_and_for_english`   | the catalog returns `None` for English and for codes it does not localize                                                                    |
| `fill_inflects_the_stub_template`                         | the template's `{name.dat}` slot renders the inflected identifier                                                                            |
| `arg_code_without_args_falls_back_to_english`             | a code whose template has `{expected}/{found}` but no args attached leaves a leftover `{`, so `localized_msg` returns `None` — the fail-safe |
| `fill_with_empty_name_leaves_no_stray_fragment`           | `fill` with an empty `name` renders cleanly — no orphaned bracket or suffix                                                                  |
| `flavor_mix_warns_only_when_tamil_meets_the_others`       | W0001 fires only when Tamil mixes with English/Tanglish (the SVO pair mixes freely)                                                          |
| `flavor_mix_warning_is_a_nonfatal_w0001`                  | the mixed-flavor diagnostic is a non-fatal W0001 warning, not an error                                                                       |

## Integration: morph (`tests/morph.rs`, 20 tests — run the real binary)

The end-to-end `--lang` path through `mimz check`/`compile`. The catalog is now
the native-authored one (33 of 36 codes, decision C3); these assert the
MECHANISM, the structured-arg interpolation, the W0001 mixed-flavor lint, and —
crucially — the **English-fallback invariant**: codes the catalog does not cover
(E0405) render byte-identically across every flavor.

| Test                                                 | Locks in                                                                                                                                                                          |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `majority_and_effective_lang_track_the_keywords`     | selection: majority + override, via the public lib API                                                                                                                            |
| `inflect_attaches_the_spec_case_suffixes`            | inflection: the four suffixes across Tamil / Tanglish / English                                                                                                                   |
| `covered_code_renders_tamil_with_the_inflected_name` | E0501 under `--lang ta` shows the localized Tamil line with `y-க்கு`                                                                                                              |
| `covered_code_auto_selects_tamil_from_the_file`      | a Tamil-keyword file with no `--lang` auto-renders E0501 in Tamil                                                                                                                 |
| `covered_code_stays_english_with_lang_en`            | `--lang en` keeps the original English wording                                                                                                                                    |
| `uncovered_code_is_identical_across_languages`       | **the fallback invariant** — E0405 is byte-identical under en / ta / tanglish                                                                                                     |
| `compile_also_localizes_diagnostics`                 | the localization path is shared — `compile --lang ta` shows Tamil E0501 too                                                                                                       |
| `unknown_lang_is_a_clean_error`                      | `--lang klingon` fails with a clear "unknown language" message                                                                                                                    |
| `e0502_renders_tamil`                                | an undriven output (E0502, a `{name}`-only template) localizes in Tamil                                                                                                           |
| `e0505_renders_tamil`                                | `=` on a reg (E0505) localizes under `--lang ta`                                                                                                                                  |
| `e0202_renders_tanglish_nameless`                    | a name-less template (E0202 const overflow) localizes with no `{name}` slot                                                                                                       |
| `e0401_interpolates_expected_and_found`              | E0401's `{expected}`/`{found}` widths interpolate; no `{token}` leaks                                                                                                             |
| `e0402_interpolates_op_lhs_rhs`                      | E0402's `{op}`/`{lhs}`/`{rhs}` (operator + both operand widths) interpolate                                                                                                       |
| `e0408_interpolates_first_and_second`                | E0408's `{first}`/`{second}` arm types interpolate (width-inferred position)                                                                                                      |
| `e0601_interpolates_type`                            | E0601's `{type}` scrutinee type interpolates on a non-exhaustive `match`                                                                                                          |
| `message_catalog_keys_are_real_checker_codes`        | every `[message.Exxxx]` key in `lang/messages.toml` is a real `ALL_CHECKER_CODES` code — a typo'd key (dead localization) fails naming it                                         |
| `message_catalog_placeholders_are_known_tokens`      | every active `{token}` in `lang/messages.toml` is one `morph::fill` fills — a typo'd placeholder / unsupplied arg would silently fall back to English forever; this fails instead |
| `mixing_tamil_with_english_warns_but_check_succeeds` | a Tamil+English file emits W0001 yet `check` still succeeds (non-fatal lint)                                                                                                      |
| `a_single_flavor_file_has_no_mix_warning`            | a clean single-flavor file does not warn                                                                                                                                          |
| `json_check_carries_the_warning_and_still_succeeds`  | `--json` includes the W0001 entry with `"severity":"warning"`, exit 0                                                                                                             |

## Integration: fmt (`tests/fmt.rs`, 9 tests — run the real binary)

`mimz fmt` — the in-place keyword-flavor normalizer (the lossless `translate`
token reskin, not the comment-dropping `--order` printer).

| Test                                              | Locks in                                                                        |
| ------------------------------------------------- | ------------------------------------------------------------------------------- |
| `normalizes_to_majority_and_is_idempotent`        | a mixed file normalizes to its majority flavor; comments survive; re-run no-ops |
| `to_flag_forces_the_target_flavor`                | `--to tamil` overrides the majority; comment preserved                          |
| `strict_warns_and_fails_on_mixed_but_still_fixes` | `--strict` warns + exits non-zero on a mixed file, still writing the fix        |
| `strict_is_clean_on_a_single_flavor_file`         | a single-flavor file passes `--strict` (no warning, exit 0)                     |
| `a_keyword_free_file_is_left_intact`              | a comment-only file (no keywords) normalizes to a no-op                         |
| `a_non_lexing_file_is_a_clean_error`              | a lex error (e.g. `/`) is reported, exits non-zero, and does not clobber input  |
| `output_flag_leaves_the_input_untouched`          | `-o <dest>` writes the result elsewhere; the input is unchanged                 |

## Unit: combinational evaluator (`src/sim/comb.rs`, 16 tests)

The Phase 1.5 simulator's combinational slice behind `mimz eval`.

| Test                                   | Locks in                                                                          |
| -------------------------------------- | --------------------------------------------------------------------------------- |
| `adder_grows_losslessly`               | `+` grows `bits[W]` → `bits[W+1]`; 200+100 carries into the 9th bit (no wrap)     |
| `wrapping_add_keeps_width`             | `+%` keeps width and wraps (300 → 44 in `bits[8]`)                                |
| `comparator_if_and_compares`           | `==`, `>`, and a value `if/else` evaluate together                                |
| `mux_match_selects`                    | `match` on `bits[2]` picks the right arm                                          |
| `chained_comparison_window`            | `lo <= value <= hi` (desugared) incl. the inclusive boundary                      |
| `rejects_sequential_logic`             | a module with `reg`/`on` is rejected with a clear message (out of the comb slice) |
| `reports_missing_input`                | a missing `--in` value names the input                                            |
| `replication_repeats_the_group`        | `{2{a}}`/`{3{a}}` repeat the group (a=0b1010 → 0xAA / 0xAAA) (A1)                 |
| `dont_care_match_picks_the_masked_arm` | `0b1??`/`0b01?`/`_` priority decoder picks the right arm per input (A2)           |
| `shift_left_zero_amt`                  | `a << 0` is identity                                                              |
| `shift_right_zero_amt`                 | `a >> 0` is identity                                                              |
| `shift_left_max_width`                 | `1 << 127` yields `2¹²⁷` (max valid shift)                                        |
| `shift_left_exceeding_width_is_zero`   | `1 << 128`, `1 << 200`, `1 << u128::MAX` → 0 (regression for the `as u32` bug)    |
| `shift_right_exceeding_width_is_zero`  | `2 >> 128`, `2 >> 200`, `2 >> u128::MAX` → 0                                      |
| `shift_left_bit_32_set_in_amt`         | `1 << (1 << 32)` → 0 (the specific `as u32` truncation trigger)                   |
| `shift_right_bit_32_set_in_amt`        | `(1 << 63) >> (1 << 32)` → 0                                                      |

## Unit: elaboration (`src/sim/elaborate.rs`, 13 tests)

Phase 1.5 steps B1 + C2–C4: flatten an AST module (and its instances) into a
`Design` (signals with folded widths, regs with folded reset + clock, comb
drivers, sequential processes), via the `elaborate_project` flattener and the
`Rw` elaborate-time rewriter (enum→index, `repeat` unroll, instance flattening,
array instances, bit-indexed drives). The event-driven kernel interprets a
`Design`.

| Test                                                | Locks in                                                                                                                                            |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `elaborates_the_counter`                            | the canonical counter flattens correctly: one reg (`value`, reset 0, clock `clk`), the `count` comb driver, `clk`/`rst` recorded, one process       |
| `param_override_folds_widths`                       | passing `WIDTH=4` folds the reg and output widths to 4                                                                                              |
| `elaborates_a_combinational_module`                 | a clockless module has empty regs/procs/clocks/resets, only comb drivers                                                                            |
| `reg_takes_a_nonzero_folded_reset_value`            | `reg r: bits[8] = 5` folds the reset to 5 and binds the reg to its `on`-block clock                                                                 |
| `flattens_a_same_file_instance`                     | C2: `Top`'s `let u = Add()` inlines the child's signals prefixed `u_*`; the `u.s` field-read resolves to the flattened `u_s` wire                   |
| `rejects_unknown_instance_module`                   | C2: a `let` instance of a module that doesn't exist is a clean "unknown module" error                                                               |
| `unrolls_repeat_with_instance_array_and_bit_drives` | C3: `repeat` inlines one child per bit (`fa__<i>`); the per-bit `s[i] = …` drives assemble into a whole-signal Concat                               |
| `elaborates_an_enum_signal_and_match`               | C4: an enum reg gets width `clog2(variants)`, its reset folds to the variant index, and a `match` over the enum elaborates (patterns → indices)     |
| `recursive_instantiation_errors_not_overflows`      | SEC-6: a self-instantiating module hits `MAX_INSTANCE_DEPTH` and errors cleanly instead of overflowing the stack                                    |
| `extreme_repeat_bounds_error_not_overflow`          | SEC-6: a `repeat` span past `i128::MAX` is an over-budget error (`checked_sub`), not an overflow panic                                              |
| `an_out_of_range_bit_index_errors`                  | SEC-6: a bit-index drive ≥ 128 errors before the `as u32` cast (no silent truncation)                                                               |
| `a_flatten_name_collision_errors`                   | SEC-6: a parent signal colliding with a flattened `inst_port` wire errors instead of silently overwriting                                           |
| `an_i128_min_const_elaborates_without_overflow`     | SEC-6 (SIM-5): a flattened child const evaluating to `i128::MIN` lowers via `unsigned_abs` instead of overflow-panicking the negation in `int_expr` |

## Unit: kernel (`src/sim/kernel.rs`, 11 tests)

Phase 1.5 step B2: the event-driven, two-phase simulation kernel that interprets
a `Design` over clock cycles (regs init to reset; each rising edge settles
combinational signals, computes next reg values, then commits all at once).
Shares the value model + expression evaluator with `comb` via `src/sim/value.rs`.

| Test                                                     | Locks in                                                                                                  |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `counter_counts_and_resets`                              | the counter counts 0→1→2→3 on rising edges; asserting `rst` forces it back to 0 (synchronous reset)       |
| `dual_edge_negedge_reg_captures_posedge_within_a_period` | a `posedge` reg feeds a `negedge` reg; the rise→fall tick lets `b` see the new `a` same period (A3)       |
| `memory_write_then_read_round_trips_a_cell`              | a `mem` cell reads init until written, then holds the clocked value; another cell still reads init (A4)   |
| `regs_init_to_their_reset_value`                         | before any tick a reg holds its (non-zero) folded reset value                                             |
| `wraps_at_declared_width`                                | `+%` on a `bits[2]` reg wraps 3→0 — width masking on the next value                                       |
| `two_phase_commit_swaps_registers`                       | `a <- b; b <- a` SWAPS (non-blocking): each reads the OLD value, proving the two-phase commit             |
| `statement_if_picks_the_next_value`                      | a statement-level `if` in the `on` block selects the reg's next value from the current state              |
| `snapshot_covers_every_signal`                           | `snapshot()` lists leaves (clk/rst/inputs), regs, and combinational outputs — the VCD/trace seam          |
| `set_rejects_a_non_leaf`                                 | driving an output or an unknown name is a clean error (only inputs/clocks/resets are drivable)            |
| `combinational_chain_propagates_in_order`                | a multi-level `wire → wire → output` chain (plus a reg input) settles in dependency order each cycle (B3) |
| `combinational_cycle_is_reported`                        | a pure comb loop (`a = b; b = a`) is caught at settle time, not spun on (the kernel's cycle guard, B3)    |

## Unit: sim runner / VCD / console trace (`src/sim/{run,vcd,trace}.rs`, 14 tests)

Phase 1.5 step B4/B5 (+ C1): the default stimulus + clocked timeline capture
(`run.rs::run`), the combinational `comb_run` (one settled frame per input
vector), the hand-written 2-state VCD writer (`vcd.rs`), and the console trace
renderer (`trace.rs`) — all over one per-cycle snapshot from the kernel.

| Test (module)                                      | Locks in                                                                            |
| -------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `counter_timeline_counts_after_reset` (run)        | the default stimulus resets cycle 0 then counts; the clock renders as a square wave |
| `inputs_are_held_for_the_run` (run)                | `--in` values hold across the whole run (`r +% x` accumulates)                      |
| `a_clockless_module_is_rejected` (run)             | the CLOCKED `run` rejects a clockless module (callers route it to `comb_run`)       |
| `an_unknown_input_is_rejected` (run)               | an unknown `--in` name is a clean error                                             |
| `comb_run_settles_one_frame_per_vector` (run)      | a combinational design settles its outputs for one input vector (lossless add)      |
| `comb_run_sweeps_a_frame_per_vector` (run)         | N input vectors → N frames, one per settle, on the clocked period                   |
| `comb_run_with_no_vectors_is_one_zero_frame` (run) | no vectors → a single all-zero-input frame                                          |
| `comb_run_rejects_a_clocked_design` (run)          | `comb_run` refuses a clocked/registered design                                      |
| `signed_lossless_add_sign_extends` (run)           | C1 regression: lossless signed `+` sign-extends a negative operand (`-2+7=5`)       |
| `header_scope_and_vars_present` (vcd)              | the VCD has `$timescale`/`$scope`/`$var`/`$enddefinitions`                          |
| `has_initial_dump_and_timestamps` (vcd)            | `$dumpvars` + `#<time>` blocks + a multi-bit `b…` vector line                       |
| `id_codes_are_unique` (vcd)                        | the base-94 signal id codes never collide                                           |
| `table_has_a_row_per_cycle` (trace)                | `--trace` renders one table row per cycle with the right count                      |
| `changes_style_omits_unchanged_frames` (trace)     | `--trace=changes` only prints when a watched signal changes (`$monitor`-style)      |

## Integration: sim (`tests/sim.rs`, 10 tests — run the real binary + lib in-process)

End-to-end `mimz sim` over a counter (clocked) and an adder (combinational): the
stimulus, the VCD, the console trace, the `--sweep`; plus the B8 kernel perf
baseline and the golden VCD byte-lock (both run the lib in-process).

| Test                                               | Locks in                                                                                                |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `trace_table_shows_a_row_per_cycle`                | `--trace` prints the per-cycle table (header + separator + N rows)                                      |
| `cycles_over_the_limit_is_rejected_by_the_cli`     | SEC: `--cycles` past `MAX_SIM_CYCLES` (1_000_000) is rejected at clap parse time — no unbounded loop    |
| `changes_trace_is_monitor_style`                   | `--trace=changes` prints `$monitor`-style lines (reaches `count=3`)                                     |
| `writes_a_gtkwave_vcd`                             | `-o` writes a VCD with `$timescale`/`$enddefinitions`/`$dumpvars`/`count`                               |
| `signals_flag_limits_the_trace`                    | `--signals count` shows only `count`, excluding `value`                                                 |
| `a_combinational_module_settles_one_frame`         | C1: a clockless module simulates — `--in a=200,b=100` → one settled frame, `sum=300`                    |
| `sweep_emits_a_frame_per_combination`              | C1: `--sweep a=1\|2\|3` (held `--in b=10`) → 3 frames, sums 11/12/13                                    |
| `a_combinational_module_writes_a_vcd`              | C1: a clockless module writes a VCD with the settled output (`sum=12`)                                  |
| `the_counter_kernel_clears_the_perf_baseline`      | the kernel sustains ≥1M cycle-events/sec on the counter in release (B8; debug uses a low sanity floor)  |
| `the_counter_vcd_matches_the_golden_byte_for_byte` | the VCD writer's exact bytes match `tests/golden/counter.vcd` (B8; `MIMZ_UPDATE_GOLDENS=1` regenerates) |

## Unit: test harness (`src/sim/harness.rs`, 6 tests)

Phase 1.5 step B6: the `test`-block runner behind `mimz test`. Runs each block
(`drive`/`tick`/`expect`/`if`) on the kernel and reports pass/fail; `tick`/`expect`
form only (the `await clk.cycles(n)` sugar awaits its native-review spelling).

| Test                                             | Locks in                                                                        |
| ------------------------------------------------ | ------------------------------------------------------------------------------- |
| `a_passing_test_counts_its_checks`               | drive/tick/expect runs in order; the `expect` count is reported                 |
| `a_failing_expect_halts_with_a_teaching_message` | a false `expect` halts the test and shows the expression + each operand's value |
| `drive_then_tick_feeds_an_input`                 | a driven input is held and accumulates across ticks                             |
| `a_test_if_branches_on_state`                    | `if`/`else` takes the live-state branch; the other branch never runs            |
| `an_unknown_clock_is_an_error`                   | `tick(<not-a-clock>)` is a setup error, not a test failure                      |
| `the_timeline_has_a_frame_per_tick`              | one trace frame per tick (+ the initial frame); default scope = interface+state |

## Integration: test (`tests/test_run.rs`, 7 tests — run the real binary)

End-to-end `mimz test`: exit codes, the teaching message, `--filter`, `--trace`,
the cycle-limit guard, and the thamizh-order test header (B7).

| Test                                                        | Locks in                                                                                                                         |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `a_passing_test_exits_zero`                                 | a passing block prints `ok` + the summary and exits 0                                                                            |
| `a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`   | SEC: `tick(clk, n)` past `MAX_SIM_CYCLES` (1_000_000) fails fast with a clean error — no untrusted-input frame-push DoS          |
| `a_failing_expect_exits_nonzero_with_a_teaching_message`    | a failing block prints `FAIL` + the expression/operands and exits 1                                                              |
| `the_filter_selects_tests_by_name`                          | `--filter` runs only the matching test (skips the failing other one)                                                             |
| `trace_shows_a_per_cycle_table`                             | `--trace` prints the per-cycle table for a test                                                                                  |
| `a_file_with_no_tests_is_reported`                          | a file with no `test` blocks reports cleanly and exits 0                                                                         |
| `a_thamizh_order_test_header_runs_like_its_code_order_twin` | a fully thamizh-order, all-tanglish program (`yetram(clk) pothu` + `M(args) kaaga "…" sodhanai`) runs and passes (the B7 oracle) |

## Integration: eval (`tests/eval.rs`, 10 tests — run the real binary)

End-to-end `mimz eval` over corpus examples — proves the lib evaluator AND the
`--in`/`--module` plumbing. The last three are security cases: the `eval` path
skips the checker, so `comb.rs` is the only overflow guard (audit SEC-2).

| Test                                        | Locks in                                                            |
| ------------------------------------------- | ------------------------------------------------------------------- |
| `adder_carries`                             | `mimz eval adder --in a=200,b=100` prints `sum = 300`               |
| `mux4_selects_with_hex_and_binary_inputs`   | `--in sel=0b10,...` parses bases; selects the right input           |
| `comparator_reports_all_three_outputs`      | all three outputs print with correct values                         |
| `window_chained_comparison_boundaries`      | inclusive boundary in / below out                                   |
| `multi_module_file_needs_module_flag`       | a 2-module file asks for `--module`, then accepts it                |
| `instances_are_rejected_clearly`            | a file with sub-module instances is rejected with a clear message   |
| `oversized_shift_const_does_not_panic`      | `a[1 << 200]` → clean overflow error, no panic/wrap (debug+release) |
| `overflowing_multiply_const_does_not_panic` | a const product past i128::MAX → overflow error, not a panic        |
| `out_of_range_index_is_rejected_cleanly`    | a literal index past the width → clean error, not a truncating cast |

## Fuzzing: `fuzz/fuzz_targets/` (CI-only, not `cargo test` units)

Four `cargo-fuzz` harnesses over the untrusted-input path, asserting the audit's
core guarantee (any byte string yields a value/Verilog or a clean `Diag`/`Err`,
never a panic / abort / hang):

- `lex_parse_eval` — NFC → `lex` → `parse` → `sim::comb::eval_outputs`, run twice
  (empty inputs for the const path, then AST-derived per-port values for the
  runtime datapath). After the random pass, 8 fixed edge-case evaluation passes
  (0, 1, u128::MAX, 1<<32, 1<<63, 1<<127, (1<<126)-1, (1<<64)-1 as all-port
  values) ensure truncation-prone boundaries are always exercised regardless of
  the random byte stream.
- `lex_parse_compile` — NFC → `lex` → `parse` → `checker::check` →
  `transliterate` → `Project::from_files` → `emit` (the Verilog backend).
- `pretty_roundtrip` — NFC → `lex` → `parse` → `pretty::pretty_print` → re-`lex`
  → re-`parse` (the printed source MUST re-parse), and for an emittable program
  the re-parsed AST must lower to byte-identical Verilog. Exercises the
  `translate --order` printer on arbitrary input (the unit suite only covers the
  fixed example corpus).
- `translate_roundtrip` — NFC → `lex` → `parse` → `translate` (keyword reskin,
  `--romanize-names`, and name-map restore): every reskin/romanize output must
  re-lex, and `romanize → restore` must be token-equivalent to the plain reskin.
  Added 2026-06-15 after a deterministic stress audit found the numeric-literal
  abutment bug (`42தொகுதி`, fixed by the `push_guarded` boundary guard).

**Not** part of the test count above: they need a nightly toolchain + libFuzzer
(Linux/macOS), live in a standalone `fuzz/` crate the root gate never builds, and
run as the CI `fuzz` job (60 s smoke per target on push/PR, corpus seeded from
`examples/`) plus a weekly `fuzz-nightly` job (10 min per target). Run locally
under WSL2/Linux with `cargo +nightly fuzz run <target>`. See
[`../audit/hardening.md`](../audit/hardening.md) "Ongoing assurance".

## Integration: grammar engine (`tests/grammar.rs`, 16 tests — run the real binary)

The `syntax thamizh` word-order profile (spec/04, Phase 1.8). Oracle = the
profile-blind backend: a thamizh-order file and its code-order twin must emit
byte-identical Verilog, so equal Verilog proves the same AST. Fixtures live in
`tests/fixtures/grammar/` (not `examples/`, which stays byte-identical
four-flavor per R9).

| Test                                                  | Locks in                                                                                               |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `thamizh_order_counter_matches_code_order_twin`       | Tanglish `rise(clk) on { }` → same Verilog as code-order twin                                          |
| `thamizh_order_tamil_counter_matches_code_order_twin` | pure Tamil script + SOV order → same Verilog as the Tamil twin                                         |
| `thamizh_order_agrees_with_english_golden`            | profile and keyword skin are fully orthogonal                                                          |
| `thamizh_order_blinker_matches_code_order_twin`       | seq conditional `<cond> enil { } illaiyenil { }` → same Verilog                                        |
| `thamizh_order_blinker_tamil_matches_code_order_twin` | the conditional flip in pure Tamil script → same Verilog                                               |
| `thamizh_order_blinker_agrees_with_english_golden`    | conditional flip is invisible to the backend (English golden)                                          |
| `thamizh_order_comparator_matches_code_order_twin`    | if-expression `c enil { } illaiyenil { }` → same Verilog                                               |
| `thamizh_order_match_matches_code_order_twin`         | match `<expr> thernthedu { }` → same Verilog (self-contained pair)                                     |
| `traffic_light_tamil_thamizh_matches_code_order_twin` | Tamil thamizh-order FSM (all four flips at once) → same Verilog; the committed `pretty`-built artifact |
| `unknown_syntax_profile_is_an_error`                  | `syntax wibble` fails to compile with E1112                                                            |
| `flipped_on_block_is_rejected_in_code_order`          | the clocked-block flip is gated on the profile                                                         |
| `flipped_conditional_is_rejected_in_code_order`       | `<cond> enil { }` rejected without the directive                                                       |
| `flipped_if_expr_is_rejected_in_code_order`           | `a > b enil { } illaiyenil { }` rejected without the directive                                         |
| `flipped_match_is_rejected_in_code_order`             | `op thernthedu { }` rejected without the directive                                                     |
| `code_order_if_is_rejected_in_thamizh`                | leading `enil` (code order) in a thamizh file errors — symmetric profile boundary                      |
| `deeply_nested_thamizh_else_if_errors_not_overflows`  | deep thamizh `illaiyenil … enil` chain → clean E1113, no stack overflow (SEC-1 guard on the flip path) |

## Deliberately NOT covered (and what would close each gap)

| Gap                                                     | Why it's open                                                                                                                                                                                                                                                                                                                                                                        | Closes when                                                 |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| Cross-INSTANCE clock-domain tracking                    | pass 6 is module-local (instance outputs carry no domain)                                                                                                                                                                                                                                                                                                                            | with the Phase 2 `sync`/multi-clock design                  |
| Diagnostic rendering format (`render`'s caret layout)   | low risk, changes are cosmetic                                                                                                                                                                                                                                                                                                                                                       | worth a snapshot test if/when output stabilizes for E-codes |
| CLI surface (`--tokens`, exit codes, `-o` default path) | thin wrappers; breakage is loud in manual use                                                                                                                                                                                                                                                                                                                                        | cheap `assert_cmd`-style tests if the CLI grows             |
| `mimz-bench` end-to-end (a full run as a test)          | it is a measuring tool over this very suite — running it under `cargo test` would re-run everything for no new assertion                                                                                                                                                                                                                                                             | if its orchestration grows logic worth locking              |
| `fmt`, grammar engine, full simulator                   | built: all five word-order flips ship (`syntax thamizh` + clocked-block, conditional, if-expression, match, test header — `tests/grammar.rs`, `tests/test_run.rs`); `translate --order` and the full event-driven simulator (`mimz sim` / `mimz test`, B1–B8) ship too, validated by the Icarus differential + the ≥1M cycle-events/sec perf baseline. Phase 1.5 is feature-complete | with their phases (1.8 / 1.5)                               |

## House rules for new tests

- New parser/emitter behavior ships with a test **in the same commit**;
  safety-rule behaviors also test the error path (message + help).
- Prefer the existing layers: table-driven facts → keyword tests; token
  shapes → lexer tests; tree shapes & teaching errors → parser tests;
  output text → integration tests on a real example.
- A new example goes into ALL FOUR flavor folders with identical
  identifiers (only keywords change — take spellings from
  `lang/keywords.toml`, never invent), plus a row in `BASE_EXAMPLES` in
  `tests/examples.rs`. `every_example_compiles` and the
  flavor-identity test then enforce it automatically.
- Update THIS page in the same session (it is the "what does a failing
  test mean" ledger — see also `tests/docs_sync.rs`, which mechanically
  guards the structural facts in these docs).
