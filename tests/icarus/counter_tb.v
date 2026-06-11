// Self-checking TB: Counter — sync reset to 0, +1 per rising edge, `+%`
// wraps 255 -> 0 (the loop runs past 256 on purpose).
`timescale 1ns/1ps
module counter_tb;
  reg clk = 0, rst = 1;
  wire [7:0] count;
  Counter dut (.clk(clk), .rst(rst), .count(count));
  always #5 clk = ~clk;

  integer i;
  initial begin
    @(posedge clk); #1; // sync reset applied on this edge
    rst = 0;
    if (count !== 8'd0) begin
      $display("FAIL: after reset count=%0d, expected 0", count);
      $finish;
    end
    for (i = 1; i <= 300; i = i + 1) begin
      @(posedge clk); #1;
      if (count !== i[7:0]) begin
        $display("FAIL: tick %0d count=%0d, expected %0d", i, count, i[7:0]);
        $finish;
      end
    end
    $display("PASS");
    $finish;
  end
endmodule
