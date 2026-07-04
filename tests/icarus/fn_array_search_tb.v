// Self-checking TB: FindIndex — proves the array-typed fn parameter
// lowering (Phase 2 array-typed fn params) drives the runtime-index mux
// (Task 9) correctly against a REAL Verilog toolchain (iverilog), not just
// this compiler's own two backends agreeing with each other.
//
// Covers both fn's compiled into the module:
//   - find_index: [a, b, c, d] passed as an array literal, indexed at each
//     of the four positions inside the fn body's guard clauses (all
//     COMPILE-TIME-CONSTANT indices — folds to plain scalar refs, no mux),
//     falling through to -1 (signed[4] two's complement) when target
//     matches none.
//   - pick: the SAME array indexed by pick_idx, a RUNTIME (non-constant)
//     index — this is the one that actually compiles to the generated
//     ternary-chain mux over vals_0..vals_3. In-range pick_idx (0..3) must
//     read back a/b/c/d respectively; pick_idx is bits[3] (0..7) but the
//     array only has 4 elements, so an out-of-range value (4..7) must fall
//     through the generated chain to the last element, d (vals_3) — per
//     spec 02 section 1.14.
`timescale 1ns/1ps
module fn_array_search_tb;
  reg [7:0] a, b, c, d, target;
  reg [2:0] pick_idx;
  wire signed [3:0] idx;
  wire [7:0] picked;
  FindIndex dut (
      .a(a),
      .b(b),
      .c(c),
      .d(d),
      .target(target),
      .pick_idx(pick_idx),
      .idx(idx),
      .picked(picked)
  );

  task check(input [7:0] xa, xb, xc, xd, xtarget, input [2:0] xpick_idx,
             input signed [3:0] xidx, input [7:0] xpicked);
    begin
      a = xa; b = xb; c = xc; d = xd; target = xtarget; pick_idx = xpick_idx; #1;
      if (idx !== xidx) begin
        $display("FAIL: find_index(%0d,%0d,%0d,%0d,%0d) -> %0d, expected %0d",
                  xa, xb, xc, xd, xtarget, idx, xidx);
        $finish;
      end
      if (picked !== xpicked) begin
        $display("FAIL: pick(%0d,%0d,%0d,%0d, idx=%0d) -> %0d, expected %0d",
                  xa, xb, xc, xd, xpick_idx, picked, xpicked);
        $finish;
      end
    end
  endtask

  initial begin
    // find_index coverage (pick_idx held at 0 -> picked == a, checked too).
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd10, 3'd0, 0, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd20, 3'd0, 1, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd30, 3'd0, 2, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd40, 3'd0, 3, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd0, -1, 8'd10);

    // pick runtime-index mux: in-range sweep (target held fixed at 99 so
    // idx stays -1 throughout, isolating the picked assertion).
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd0, -1, 8'd10);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd1, -1, 8'd20);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd2, -1, 8'd30);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd3, -1, 8'd40);

    // pick out-of-range fallback: any pick_idx >= 4 must read back the
    // last element (d) per the generated mux's own default-chain shape.
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd4, -1, 8'd40);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd5, -1, 8'd40);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, 3'd7, -1, 8'd40);

    $display("PASS");
    $finish;
  end
endmodule
