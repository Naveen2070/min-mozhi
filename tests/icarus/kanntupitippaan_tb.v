// Self-checking TB: kanntupitippaan — the pure-Tamil twin of
// `fn_return_guard_tb.v` (english/fn_return_guard.mimz ->
// tamil-pure/fn_return_guard.mimz). Same guard-clause `if`/`return`
// coverage (lowest set bit, or -1 when none), through the romanized
// identifiers: a/idx -> a/itam.
`timescale 1ns/1ps
module kanntupitippaan_tb;
  reg [7:0] a;
  wire signed [3:0] itam;
  kanntupitippaan dut (.a(a), .itam(itam));

  task check(input [7:0] xa, input signed [3:0] xitam);
    begin
      a = xa; #1;
      if (itam !== xitam) begin
        $display("FAIL: kanntupiti(%b) -> %0d, expected %0d", xa, itam, xitam);
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
