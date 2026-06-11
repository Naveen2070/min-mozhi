// Self-checking TB: Chained — the 2-bit ripple-carry adder built from
// FullAdder instances; the full 32-row truth table.
`timescale 1ns/1ps
module chained_tb;
  reg a0, a1, b0, b1, cin;
  wire sum0, sum1, cout;
  Chained dut (.a0(a0), .a1(a1), .b0(b0), .b1(b1), .cin(cin),
               .sum0(sum0), .sum1(sum1), .cout(cout));

  integer i;
  reg [2:0] want;
  initial begin
    for (i = 0; i < 32; i = i + 1) begin
      {cin, b1, b0, a1, a0} = i[4:0]; #1;
      want = {a1, a0} + {b1, b0} + cin;
      if ({cout, sum1, sum0} !== want) begin
        $display("FAIL: a=%b%b b=%b%b cin=%b -> %b%b%b, expected %b",
                 a1, a0, b1, b0, cin, cout, sum1, sum0, want);
        $finish;
      end
    end
    $display("PASS");
    $finish;
  end
endmodule
