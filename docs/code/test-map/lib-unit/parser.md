# Unit: parser (`crates/mimz-core/src/parser/tests/`, 102 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Split 2026-07-26 (`oversized-test-file-split`) from a single 1594-line
`tests.rs` into 13 topic files under `tests/`; `mod.rs` keeps only the
shared `parse_ok`/`parse_err`/`parse_expr_ok` helpers. Zero test-behavior
change — every row below is the same test that existed before, just
organized by file.

## parser/tests/bundles.rs (5 tests)

| Test                                 | Locks in                                                                                 |
| ------------------------------------ | ---------------------------------------------------------------------------------------- |
| `parse_bundle_decl`                  | `bundle` struct declarations parse with fields                                           |
| `parse_bundle_as_port_type`          | a bundle type used as a port type (bare or with args, e.g. `Hs(X: 1)`) parses            |
| `parse_bundle_literal`               | bundle literals `Bundle { f: x }` parse                                                  |
| `parse_bundle_destructure`           | bundle destructuring `let { f } = b` parses                                              |
| `parse_bundle_field_rename_is_error` | `let { valid: v } = bus` (renaming a destructured field) is E0904, not silently accepted |

## parser/tests/calls_and_modules.rs (6 tests)

| Test                                    | Locks in                                                                                            |
| --------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `builtin_with_wrong_arity_is_e1110`     | a built-in called with the wrong argument count (e.g. `min(a)`) is E1110                            |
| `non_builtin_call_parses_as_fncall`     | a call to a non-builtin name (`mac(x, y)`) parses as `ExprKind::FnCall`, not a builtin `Call`       |
| `builtin_call_still_parses_as_builtin`  | a call to a builtin name (`extend(x, 8)`) still parses as `ExprKind::Call`, not swept into `FnCall` |
| `zero_arg_call_parses_as_fncall`        | a zero-argument call (`foo()`) parses as `FnCall` with an empty arg list                            |
| `parses_counter`                        | the canonical example parses; module has the expected 6 items                                       |
| `parses_tanglish_counter_to_same_shape` | Tanglish source → structurally identical AST (the thesis, AST level)                                |

## parser/tests/enums_and_tagged_unions.rs (6 tests)

| Test                                                        | Locks in                                                                                                |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `tagged_enum_parses`                                        | enum with payload fields parses correctly (Phase 2)                                                     |
| `mixed_tag_only_and_tagged_parses`                          | an enum mixing tag-only and payload-bearing variants in one declaration parses                          |
| `match_with_payload_bindings_parses`                        | `match` arms with payload bindings `Variant(x, y)` parse                                                |
| `enum_construct_parses_with_payload_args`                   | `Packet.Ctrl(k)` parses to `ExprKind::EnumConstruct` with the variant name and args                     |
| `enum_construct_parses_with_zero_args_for_tag_only_variant` | `State.Idle()` (explicit empty parens on a tag-only variant) parses to `EnumConstruct` with zero args   |
| `bare_enum_variant_reference_still_parses_as_field`         | `State.Idle` with no trailing `()` stays `ExprKind::Field`, not swept into `EnumConstruct` (regression) |

## parser/tests/extern_module_and_sync_builtins.rs (6 tests)

| Test                                                   | Locks in                                                                                                   |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `extern_module_parses_with_params_doc_and_ports`       | `extern module Pll(MULT: int = 2) { doc: "..." ... }` parses to `ExternModule` with params, doc, and ports |
| `extern_module_parses_with_alias_and_no_params_or_doc` | `extern module Pll = "PLL_HARD_IP_v2" { }` parses with the Verilog alias name, no params, no doc           |
| `extern_module_body_rejects_wire_declarations`         | an `extern module` body containing a `wire` declaration is a parse error — only ports are allowed          |
| `sync_double_flop_call_parses_as_a_builtin_call`       | `sync.double_flop(fast_bit, clk_src, clk_dst)` parses as `Builtin::SyncDoubleFlop` with 3 args             |
| `sync_pulse_call_parses_as_a_builtin_call`             | `sync.pulse(src_pulse, clk_src, clk_dst)` parses as `Builtin::SyncPulse` with 3 args                       |
| `sync_dot_with_unknown_method_is_a_clean_parse_error`  | `sync.nonsense(...)` (an unknown `sync.*` method) is a clean E1116, never a panic                          |

## parser/tests/fn_decl_thamizh_and_stmts.rs (3 tests)

| Test                              | Locks in                                                                                                   |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `fn_decl_parses_in_thamizh_order` | `fn` declarations are code-order-only (no SOV flip) — a `syntax thamizh` file still accepts a leading `fn` |
| `parse_default_stmt`              | `default` assignment statements parse inside `on`                                                          |
| `parse_const_if_block`            | `const if` elaboration blocks parse                                                                        |

## parser/tests/fn_decls.rs (5 tests)

| Test                                                  | Locks in                                                                                                                                                       |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `parses_fn_with_local_let_and_body`                   | Phase 2 `fn` with `let` locals and a final block return parses                                                                                                 |
| `parses_fn_with_guard_clause_return`                  | `fn` with `return` statement guard clause parses                                                                                                               |
| `parses_fn_with_thamizh_order_guard_clause_return`    | thamizh word-order guard clause (`<cond> enil { thirumbu ... }`) parses to the same shape as code order — `return`/`thirumbu` stays prefix-only in both orders |
| `parses_fn_with_if_else_stmt`                         | a statement-level `if`/`else` inside a `fn` body parses with both branches populated                                                                           |
| `parses_fn_with_only_locals_and_tail_backward_compat` | the pre-Phase-2 `fn` shape (locals + tail expr, no `if`/`return`) still parses — the backward-compat contract                                                  |

## parser/tests/item_grammar.rs (15 tests)

| Test                                             | Locks in                                                                                                                      |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `on_fall_parses_with_the_fall_edge`              | `on fall(clk)` parses to `OnBlock` with `Edge::Fall` (A3)                                                                     |
| `mem_declaration_parses_to_a_mem_item`           | `mem m: bits[8][4] = 0` parses to `ModuleItem::Mem` (name/ty/depth/init) (A4)                                                 |
| `a_mem_without_an_init_value_is_e1104`           | a `mem` missing its `= init` is E1104 (no uninitialized state), like a reg (A4)                                               |
| `array_type_parses_in_a_fn_param`                | an array-typed `fn` parameter (`vals: bits[8][4]`) parses to `Type::Array`                                                    |
| `assert_parses_in_a_module_body`                 | a module-body `assert(a)` parses to `ModuleItem::Assert`                                                                      |
| `assert_with_a_message_parses_in_a_module_body`  | `assert(a, "msg")` carries the message string on the node                                                                     |
| `assert_message_must_be_a_string_literal`        | `assert(a, a)` (a non-literal message) is E1101 with a help line                                                              |
| `cover_parses_in_a_module_body`                  | a module-body `cover(a)` parses to `ModuleItem::Cover`                                                                        |
| `cover_with_a_label_parses_in_a_module_body`     | `cover(a, "label")` carries the label string on the node                                                                      |
| `cover_label_must_be_a_string_literal`           | `cover(a, a)` (a non-literal label) is E1101 with a help line                                                                 |
| `assert_parses_inside_an_on_block`               | `assert(a)` inside `on rise(clk)` parses to `SeqStmt::Assert`                                                                 |
| `assert_parses_inside_an_on_block_thamizh_order` | `assert` stays keyword-first in thamizh word order (not a clause head, no SOV flip)                                           |
| `cover_parses_inside_an_on_block`                | `cover(a)` inside `on rise(clk)` parses to `SeqStmt::Cover`                                                                   |
| `cover_parses_inside_an_on_block_thamizh_order`  | `cover` likewise stays keyword-first in thamizh word order                                                                    |
| `nested_array_type_parses_two_brackets_deep`     | a doubly-bracketed array type (`bits[8][4][2]`) parses without ambiguity — the CHECKER, not the parser, rejects nested arrays |

## parser/tests/module_refs_and_arrays.rs (5 tests)

| Test                                                 | Locks in                                                                                                 |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `qualified_module_reference_parses`                  | `a.b.Foo() { }` parses to an `Inst` with a 2-segment qualified path                                      |
| `bare_module_reference_still_parses_with_empty_path` | `Foo() { }` with no qualifying path parses with `inst.module.is_bare()` true                             |
| `array_literal_parses`                               | `[1, 2, 3, 4]` parses to `ExprKind::ArrayLit` with 4 elements                                            |
| `empty_array_literal_parses`                         | `[]` parses to an empty `ArrayLit` — the parser accepts it; the checker later rejects zero-length arrays |
| `array_literal_as_fn_call_argument_parses`           | an array literal passed as a `fn` call argument (`f([1, 2, 3, 4])`) parses                               |

## parser/tests/repeat_loop_foreach.rs (9 tests)

| Test                                                  | Locks in                                                                        |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- |
| `parses_repeat_and_const`                             | `repeat i: 0..8` and file-level `const` parse                                   |
| `parses_loop_inside_on_block`                         | `loop i in 0..4` inside `on` block parses                                       |
| `sync_loop_parses`                                    | `sync loop` constructs parse with variables, boundaries, and result types       |
| `parses_loop_inside_fn_body`                          | `loop` inside combinational `fn` block parses                                   |
| `foreach_range_form_parses_as_module_item`            | `foreach i in 0..4 { }` at module-item level parses to `ForEach` (range form)   |
| `foreach_elements_form_parses_as_module_item`         | `foreach v in arr { }` at module-item level parses to `ForEach` (elements form) |
| `foreach_parses_inside_on_block`                      | `foreach` inside a clocked `on` block parses                                    |
| `foreach_parses_inside_fn_body`                       | `foreach` inside a combinational `fn` block parses                              |
| `foreach_elements_form_rejects_non_identifier_source` | `foreach v in <non-identifier expr>` is a parse error, not a silent misparse    |

## parser/tests/reset_and_thamizh_order.rs (10 tests)

| Test                                                        | Locks in                                                                             |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `async_reset_parses_with_the_async_flag`                    | `async reset rst` sets `Reset.is_async` (A5)                                         |
| `a_plain_reset_is_synchronous`                              | a bare `reset rst` leaves `is_async` false — sync is the default (A5)                |
| `thamizh_order_on_fall_parses_to_the_fall_edge`             | `irakkam(clk) pothu { }` → the same fall block (thamizh order) (A3)                  |
| `thamizh_order_on_block_parses_to_the_same_shape`           | `syntax thamizh` + `yetram(clk) pothu { }` → the same module (spec/04)               |
| `english_syntax_thamizh_directive_also_selects_the_profile` | flavor and word-order profile are orthogonal (`syntax thamizh` in English)           |
| `unknown_syntax_profile_is_e1112`                           | `syntax wibble` → E1112, not silently ignored                                        |
| `flipped_on_block_needs_the_directive`                      | a leading `rise(...)` is a parse error without the directive (gated flip)            |
| `thamizh_order_test_header_parses_to_the_same_shape`        | `M(args) kaaga "…" sodhanai { }` → the SAME `TestDecl` as the code-order header (B7) |
| `thamizh_test_header_with_no_params_parses`                 | the flipped test header with no params (`Counter kaaga "…" sodhanai`) parses         |
| `the_test_header_flip_needs_the_directive`                  | a leading identifier test header without `syntax thamizh` is E1102 (gated flip)      |

## parser/tests/safety_and_precedence.rs (18 tests)

| Test                                                               | Locks in                                                                                          |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `deeply_nested_expression_errors_not_overflows`                    | `(((…)))` past the depth cap → clean E1113, not a stack overflow (SEC-1)                          |
| `deeply_nested_unary_errors_not_overflows`                         | `!!!!…x` prefix chain → E1113 via the `unary` guard, not a crash                                  |
| `a_long_flat_binary_chain_parses_without_tripping_the_depth_guard` | a 5000-term `a + a + …` chain parses — LENGTH is unbounded, distinct from nesting DEPTH           |
| `stray_top_level_brace_does_not_hang`                              | a stray top-level `}` errors and terminates — `file()` cannot spin (OOM)                          |
| `rust_precedence_defuses_the_c_trap`                               | `x & 1 == 0` parses as `(x & 1) == 0` — **never** change this                                     |
| `monotonic_chained_comparison_desugars_to_and`                     | `0 <= x <= 7` desugars to `(0<=x) && (x<=7)` — the safe Python form (8.9)                         |
| `qq_parses_as_lowest_precedence_left_associative`                  | `a \|\| b ?? c` parses as `(a \|\| b) ?? c` — `??` binds LOOSER than `\|\|`                       |
| `qq_chain_is_left_associative`                                     | `a ?? b ?? c` reads `(a ?? b) ?? c` — left-associative chaining                                   |
| `replication_parses_to_replicate`                                  | `{2{a}}` parses as `Replicate` (count + inner parts), not concatenation (A1)                      |
| `braces_without_an_inner_group_stay_concat`                        | `{a, a}` still parses as `Concat` — the replication path is no regression                         |
| `dont_care_pattern_parses_to_intmask`                              | `0b1??` in a match arm parses as `Pattern::IntMask` (value/mask/width) (A2)                       |
| `mixed_direction_chain_is_an_error`                                | `a < b > c` stays E1109 (the confusing form)                                                      |
| `equality_cannot_be_chained`                                       | `a == b == c` stays E1109                                                                         |
| `wire_if_without_else_teaches_about_latches`                       | mandatory `else` on if-expressions + the latch help text                                          |
| `reg_without_reset_value_is_an_error`                              | mandatory reg reset (safety rule)                                                                 |
| `assign_arrow_confusion_teaches`                                   | `=` inside `on` → help text pointing to `<-`                                                      |
| `every_parse_error_carries_a_code`                                 | the E11xx retrofit, locked from outside: no parse error is codeless                               |
| `every_parse_error_carries_a_help_line`                            | GAP-3: every E11xx code sweeps to a mandatory help line, same teaching contract as `Checker::err` |

## parser/tests/test_blocks_sim_and_recovery.rs (9 tests)

| Test                                                   | Locks in                                                                                                                      |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `parses_test_block`                                    | `test "..." for M(...) { tick/expect }` parses                                                                                |
| `empty_parens_variant_is_a_parse_error`                | `enum Foo { A() }` (empty-parens tag-only variant) is E1113 with a "tag-only" hint — parens must be omitted, not empty        |
| `parse_recover_keeps_good_items_around_a_bad_one`      | `parse_recover` leaves one `ModuleItem::Error` for a bad line; both ports survive                                             |
| `parse_recover_top_level_error_keeps_following_module` | file-level garbage becomes `TopItem::Error`; the next module still parses                                                     |
| `parse_recover_seq_and_test_blocks_emit_error_nodes`   | a bad stmt in `on`/`test` yields `Seq`/`TestStmt::Error`; good stmts survive                                                  |
| `sim_block_parses`                                     | a `test` block's `sim { speed mhz(50) bind playing -> led(color: green) }` parses — speed clause + one bind with an ident arg |
| `sim_block_bind_arg_accepts_a_bare_integer`            | a `sim` bind arg accepts a bare integer (`bind tx -> uart_tx(baud: 9600)`), not just an identifier                            |
| `sim_block_bad_syntax_recovers`                        | bad syntax inside a `sim { }` block recovers to a `TestStmt::Error`; the surrounding `tick` still parses                      |
| `strict_parse_still_errs_on_bad_input`                 | the strict `parse` contract is unchanged — any error discards the tree                                                        |

## parser/tests/valid_bundle_sugar.rs (5 tests)

| Test                                                                            | Locks in                                                                                                                                              |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bit_question_desugars_to_builtin_valid_bundle`                                 | `bit?` desugars to the built-in `__Valid` bundle type with `N=1`                                                                                      |
| `bits_n_question_desugars_with_the_width_expr`                                  | `bits[8]?` desugars to `__Valid` with `N` bound to the width expression (8)                                                                           |
| `signed_n_question_desugars_to_valid_signed`                                    | `signed[8]?` desugars to the built-in `__ValidSigned` bundle type with `N=8`                                                                          |
| `double_question_on_a_type_is_rejected`                                         | `bits[8]??` (double `?`) is E1115, not silently accepted                                                                                              |
| `mem_declaration_still_parses_to_the_same_shape_after_array_type_grammar_lands` | `mem`'s own declaration grammar is unaffected by the `T?`/array-type grammar work — `ty` stays a scalar, `depth` a separate `Expr` (regression guard) |

The error-path tests assert on message/help **substrings** (loose, so
wording can be polished) AND on the stable E-code (tight — the
contract). Lexer error tests do the same with E10xx.
