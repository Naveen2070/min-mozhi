// Self-checking TB: FindFirstSet — proves the guard-clause `if`/`return`
// lowering (Phase 2 statement-based fn bodies) finds the lowest set bit,
// one guard per bit position, falling through to -1 (signed[4] two's
// complement) when no bit is set. Exhaustive over the 8 single-bit-set
// inputs plus the all-zero fallthrough case.
`timescale 1ns/1ps
module fn_return_guard_tb;
  reg [7:0] a;
  wire signed [3:0] idx;
  FindFirstSet dut (.a(a), .idx(idx));

  task check(input [7:0] xa, input signed [3:0] xidx);
    begin
      a = xa; #1;
      if (idx !== xidx) begin
        $display("FAIL: find_first_set(%b) -> %0d, expected %0d", xa, idx, xidx);
        $finish;
      end
    end
  endtask

  initial begin
    check(8'b00000000, -1);
    check(8'b00000001, 0);
    check(8'b00000010, 1);
    check(8'b00000100, 2);
    check(8'b00001000, 3);
    check(8'b00010000, 4);
    check(8'b00100000, 5);
    check(8'b01000000, 6);
    check(8'b10000000, 7);
    $display("PASS");
    $finish;
  end
endmodule
