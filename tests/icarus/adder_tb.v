// Self-checking TB: Adder — lossless `+` (the 9th bit carries, never drops).
`timescale 1ns/1ps
module adder_tb;
  reg [7:0] a, b;
  wire [8:0] sum;
  Adder dut (.a(a), .b(b), .sum(sum));

  task check(input [7:0] xa, input [7:0] xb);
    begin
      a = xa; b = xb; #1;
      if (sum !== {1'b0, xa} + {1'b0, xb}) begin
        $display("FAIL: %0d + %0d = %0d (expected %0d)", xa, xb, sum, xa + xb);
        $finish;
      end
    end
  endtask

  integer i;
  initial begin
    check(8'd0, 8'd0);
    check(8'd255, 8'd1);   // the classic dropped carry: needs bit 8
    check(8'd255, 8'd255);
    check(8'd100, 8'd55);
    for (i = 0; i < 50; i = i + 1) check(i * 37, i * 91);
    $display("PASS");
    $finish;
  end
endmodule
