// Self-checking TB: ShiftRegister — bits enter at the LSB and march left;
// after 8 ticks the register holds the fed pattern, first bit at the MSB.
`timescale 1ns/1ps
module shift_register_tb;
  reg clk = 0, rst = 1, din = 0;
  wire [7:0] dout;
  ShiftRegister dut (.clk(clk), .rst(rst), .din(din), .dout(dout));
  always #5 clk = ~clk;

  reg [7:0] pattern;
  integer i;
  initial begin
    pattern = 8'b10110011;
    @(posedge clk); #1; rst = 0;
    if (dout !== 8'd0) begin
      $display("FAIL: after reset dout=%b, expected 0", dout);
      $finish;
    end
    for (i = 7; i >= 0; i = i - 1) begin
      din = pattern[i]; // MSB first
      @(posedge clk); #1;
    end
    if (dout !== pattern) begin
      $display("FAIL: dout=%b, expected %b", dout, pattern);
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule
