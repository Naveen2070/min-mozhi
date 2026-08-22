# Integration: extern module / Verilog FFI (`tests/extern.rs`, 5 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

`extern module` declares a Verilog module we do NOT compile - a black box
(vendor IP, a hand-written primitive). The compiler still type-checks the
port list and instantiates it; the simulator needs to be told what to do
with it.

| Test                                                      | Locks in                                                                                |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `extern_module_checks_clean`                              | a well-formed `extern module` passes `mimz check`                                       |
| `extern_module_compiles_to_instantiation_only`            | the output instantiates it and never emits a definition                                 |
| `extern_module_alias_uses_real_verilog_name`              | `verilog "RealName"` puts the vendor's own name in the output                           |
| `extern_sim_strict_flag_makes_mimz_test_fail_fast`        | `--extern-sim strict` refuses to simulate a black box (`S0113`) instead of guessing     |
| `extern_src_cli_flag_unions_with_mimz_toml_verilog_files` | `--extern-src` on the CLI ADDS to `mimz.toml`'s `verilog_files`, it does not replace it |
