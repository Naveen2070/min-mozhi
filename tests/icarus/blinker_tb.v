// Self-checking TB: Blinker — LIMIT is a parameter, so the testbench
// instantiates #(.LIMIT(3)) and sees the toggle in 4 ticks instead of
// simulating 50 million cycles.
`timescale 1ns/1ps
module blinker_tb;
  reg clk = 0, rst = 1;
  wire led;
  Blinker #(.LIMIT(3)) dut (.clk(clk), .rst(rst), .led(led));
  always #5 clk = ~clk;

  task tick;
    begin
      @(posedge clk); #1;
    end
  endtask

  task check(input want, input [31:0] at);
    begin
      if (led !== want) begin
        $display("FAIL: tick %0d led=%b, expected %b", at, led, want);
        $finish;
      end
    end
  endtask

  integer i;
  initial begin
    tick; rst = 0; check(1'b0, 0);
    for (i = 1; i <= 3; i = i + 1) begin
      tick; check(1'b0, i); // cnt counting 1, 2, 3
    end
    tick; check(1'b1, 4);   // cnt hit LIMIT -> state toggles
    for (i = 5; i <= 7; i = i + 1) begin
      tick; check(1'b1, i);
    end
    tick; check(1'b0, 8);   // and toggles back
    $display("PASS");
    $finish;
  end
endmodule
