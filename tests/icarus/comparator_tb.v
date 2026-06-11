// Self-checking TB: Comparator — eq/gt comparisons and the if-expression
// driving `max` (both branches always exist; no latch possible).
`timescale 1ns/1ps
module comparator_tb;
  reg [7:0] a, b;
  wire eq, gt;
  wire [7:0] max;
  Comparator dut (.a(a), .b(b), .eq(eq), .gt(gt), .max(max));

  task check(input [7:0] xa, input [7:0] xb, input xeq, input xgt, input [7:0] xmax);
    begin
      a = xa; b = xb; #1;
      if (eq !== xeq || gt !== xgt || max !== xmax) begin
        $display("FAIL: a=%0d b=%0d -> eq=%b gt=%b max=%0d", xa, xb, eq, gt, max);
        $finish;
      end
    end
  endtask

  initial begin
    check(8'd3, 8'd5, 1'b0, 1'b0, 8'd5);
    check(8'd7, 8'd7, 1'b1, 1'b0, 8'd7);
    check(8'd9, 8'd2, 1'b0, 1'b1, 8'd9);
    check(8'd0, 8'd255, 1'b0, 1'b0, 8'd255);
    check(8'd255, 8'd0, 1'b0, 1'b1, 8'd255);
    $display("PASS");
    $finish;
  end
endmodule
