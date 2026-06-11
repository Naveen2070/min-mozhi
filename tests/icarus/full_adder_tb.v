// Self-checking TB: FullAdder — the 8-row truth table.
`timescale 1ns/1ps
module full_adder_tb;
  reg a, b, cin;
  wire sum, cout;
  FullAdder dut (.a(a), .b(b), .cin(cin), .sum(sum), .cout(cout));

  integer i;
  initial begin
    for (i = 0; i < 8; i = i + 1) begin
      {a, b, cin} = i[2:0]; #1;
      if ({cout, sum} !== a + b + cin) begin
        $display("FAIL: a=%b b=%b cin=%b -> sum=%b cout=%b", a, b, cin, sum, cout);
        $finish;
      end
    end
    $display("PASS");
    $finish;
  end
endmodule
