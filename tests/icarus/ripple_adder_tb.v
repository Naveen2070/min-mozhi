// Self-checking TB: RippleAdder — the WIDTH=4 ripple-carry adder built by
// unrolling `repeat` over a chain of FullAdder instances. Exhaustive: all
// 512 combinations of a (4b), b (4b), and cin (1b). The 5-bit result
// {cout, sum} must equal a + b + cin for every one.
`timescale 1ns/1ps
module ripple_adder_tb;
  reg  [3:0] a, b;
  reg        cin;
  wire [3:0] sum;
  wire       cout;
  RippleAdder dut (.a(a), .b(b), .cin(cin), .sum(sum), .cout(cout));

  integer i;
  reg [4:0] want;
  initial begin
    for (i = 0; i < 512; i = i + 1) begin
      {cin, b, a} = i[8:0]; #1;
      want = a + b + cin;               // 5-bit: carry kept
      if ({cout, sum} !== want) begin
        $display("FAIL: a=%0d b=%0d cin=%b -> {cout,sum}=%b, expected %b",
                 a, b, cin, {cout, sum}, want);
        $finish;
      end
    end
    $display("PASS");
    $finish;
  end
endmodule
