// Self-checking TB: EdgeDetector — pulse is HIGH only while din is 1 and
// last cycle's value was 0: exactly one cycle per rising edge of din.
`timescale 1ns/1ps
module edge_detector_tb;
  reg clk = 0, rst = 1, din = 0;
  wire pulse;
  EdgeDetector dut (.clk(clk), .rst(rst), .din(din), .pulse(pulse));
  always #5 clk = ~clk;

  task check(input want, input [31:0] tag);
    begin
      if (pulse !== want) begin
        $display("FAIL: case %0d pulse=%b, expected %b", tag, pulse, want);
        $finish;
      end
    end
  endtask

  initial begin
    @(posedge clk); #1; rst = 0;
    check(1'b0, 1);     // din=0, prev=0: idle
    din = 1; #1;
    check(1'b1, 2);     // rising edge seen combinationally
    @(posedge clk); #1; // prev catches up
    check(1'b0, 3);     // steady 1: pulse is over
    @(posedge clk); #1;
    check(1'b0, 4);
    din = 0; #1;
    check(1'b0, 5);     // falling edge: no pulse
    @(posedge clk); #1; // prev -> 0
    din = 1; #1;
    check(1'b1, 6);     // second rising edge pulses again
    $display("PASS");
    $finish;
  end
endmodule
