// Self-checking TB: minukki — the pure-Tamil PWM (examples/tamil-pure), the
// same circuit as Pwm with Tamil names. Ports are the romanized Tamil names:
// clk=katikai, rst=miill, duty=katamai, pwm=alai. At WIDTH=4 exactly `duty` of
// every 16 consecutive cycles are high.
`timescale 1ns/1ps
module minukki_tb;
  reg clk = 0, rst = 1;
  reg [3:0] duty;
  wire pwm;
  minukki #(.akalam(4)) dut (.katikai(clk), .miill(rst), .katamai(duty), .alai(pwm));
  always #5 clk = ~clk;

  integer i, high;
  initial begin
    duty = 0;
    @(posedge clk); #1; rst = 0;

    for (i = 0; i < 20; i = i + 1) begin
      @(posedge clk); #1;
      if (pwm !== 1'b0) begin
        $display("FAIL: duty=0 pwm=%b at i=%0d, expected 0", pwm, i);
        $finish;
      end
    end

    duty = 5;
    @(posedge clk); #1;
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
