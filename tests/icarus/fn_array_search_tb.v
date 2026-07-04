// Self-checking TB: FindIndex — proves the array-typed fn parameter
// lowering (Phase 2 array-typed fn params) drives the runtime-index mux
// (Task 9) correctly: [a, b, c, d] passed as an array literal, indexed at
// each of the four positions inside the fn body's guard clauses, falling
// through to -1 (signed[4] two's complement) when target matches none.
`timescale 1ns/1ps
module fn_array_search_tb;
  reg [7:0] a, b, c, d, target;
  wire signed [3:0] idx;
  FindIndex dut (.a(a), .b(b), .c(c), .d(d), .target(target), .idx(idx));

  task check(input [7:0] xa, xb, xc, xd, xtarget, input signed [3:0] xidx);
    begin
      a = xa; b = xb; c = xc; d = xd; target = xtarget; #1;
      if (idx !== xidx) begin
        $display("FAIL: find_index(%0d,%0d,%0d,%0d,%0d) -> %0d, expected %0d",
                  xa, xb, xc, xd, xtarget, idx, xidx);
        $finish;
      end
    end
  endtask

  initial begin
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd10, 0);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd20, 1);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd30, 2);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd40, 3);
    check(8'd10, 8'd20, 8'd30, 8'd40, 8'd99, -1);
    $display("PASS");
    $finish;
  end
endmodule
