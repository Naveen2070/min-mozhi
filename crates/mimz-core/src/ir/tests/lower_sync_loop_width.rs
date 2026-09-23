use super::lower_valid;

/// GAP-1: `sync_loop_lower.rs`'s desugared counter increment (`cnt <- cnt +
/// 1`) used plain `BinOp::Add`, which grows the result one bit past the
/// counter's own declared `clog2(hi)` width — a `Dff` whose `d` pin (the
/// grown Add) is wider than its own `q` pin (the register's declared
/// width). The counter never actually needs the extra bit (the increment
/// only runs when `cnt != hi - 1`, so it never overflows in practice), so
/// `+%` (same-width wraparound, never actually wrapping here) is exactly
/// the right operator — matching the checker's own E0401 guidance for
/// hand-written code hitting this identical shape ("For same-width
/// wrap-around use `+%`/`-%`").
#[test]
fn sync_loop_counter_increment_does_not_grow_past_its_declared_width() {
    let src = r#"
        module M {
          clock clk
          reset rst
          mem m: bits[8][8] = 0
          in key: bits[8]
          out found: signed[4]
          out busy: bit
          sync loop find_first on rise(clk) (i: 0..8) -> result: signed[4] = 0 - 1 {
            if m[i] == key { result <- signed({false, i}) }
          }
          found = find_first_result
          busy = find_first_running
        }
    "#;
    let _module = lower_valid(src);
}
