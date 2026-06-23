module pwm_holds_low_at_zero_duty_tb;
  reg clk;
  reg rst;
  reg [4-1:0] duty;
  wire pwm;

  Pwm  #(.WIDTH(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .duty(duty),
    .pwm(pwm)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("pwm_holds_low_at_zero_duty_tb.vcd");
    $dumpvars(0, pwm_holds_low_at_zero_duty_tb);
    rst = 0;
    duty = 0;
    duty = 0;
    repeat (6) @(posedge clk);
    if (!((pwm == 0))) begin
      $display("FAIL: expect %0s failed", "(pwm == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module pwm_leads_each_period_high_for_non_zero_duty_tb;
  reg clk;
  reg rst;
  reg [4-1:0] duty;
  wire pwm;

  Pwm  #(.WIDTH(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .duty(duty),
    .pwm(pwm)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("pwm_leads_each_period_high_for_non_zero_duty_tb.vcd");
    $dumpvars(0, pwm_leads_each_period_high_for_non_zero_duty_tb);
    rst = 0;
    duty = 0;
    duty = 8;
    if (!((pwm == 1))) begin
      $display("FAIL: expect %0s failed", "(pwm == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

