// Self-checking TB: Pwm — a free-running counter with pwm = counter < duty.
// At WIDTH=4 the period is 16 counts. duty=0 holds pwm low forever; for any
// duty, exactly `duty` of every 16 consecutive cycles are high (the counter
// visits each value once per period). Sync reset clears the counter to 0.
`timescale 1ns/1ps
module std_pwm_tb;
  reg clk = 0, rst = 1;
  reg [3:0] duty;
  wire pwm;
  Pwm #(.WIDTH(4)) dut (.clk(clk), .rst(rst), .duty(duty), .pwm(pwm));
  always #5 clk = ~clk;

  integer i, high;
  initial begin
    duty = 0;
    @(posedge clk); #1; rst = 0; // counter starts at 0 after the sync reset

    // duty = 0 → the output is never driven high.
    for (i = 0; i < 20; i = i + 1) begin
      @(posedge clk); #1;
      if (pwm !== 1'b0) begin
        $display("FAIL: duty=0 pwm=%b at i=%0d, expected 0", pwm, i);
        $finish;
      end
    end

    // Over one full 16-count period exactly `duty` cycles are high.
    duty = 5;
    @(posedge clk); #1; // settle one edge with the new duty
    high = 0;
    for (i = 0; i < 16; i = i + 1) begin
      @(posedge clk); #1;
      if (pwm === 1'b1) high = high + 1;
    end
    if (high !== 5) begin
      $display("FAIL: duty=5 high-count=%0d over one period, expected 5", high);
      $finish;
    end

    $display("PASS");
    $finish;
  end
endmodule
