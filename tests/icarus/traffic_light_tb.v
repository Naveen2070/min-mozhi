// Self-checking TB: TrafficLight FSM. The timer arm is matched on the
// OLD state (non-blocking `<-`), so leaving Red loads 50, leaving Green
// loads 40, leaving Yellow loads 10 — the checkpoints below encode that.
`timescale 1ns/1ps
module traffic_light_tb;
  reg clk = 0, rst = 1;
  wire red, yellow, green;
  TrafficLight dut (.clk(clk), .rst(rst), .red(red), .yellow(yellow), .green(green));
  always #5 clk = ~clk;

  task tick;
    begin
      @(posedge clk); #1;
    end
  endtask

  task check(input xr, input xy, input xg, input [31:0] at);
    begin
      if (red !== xr || yellow !== xy || green !== xg) begin
        $display("FAIL: tick %0d r/y/g=%b%b%b, expected %b%b%b", at, red, yellow, green, xr, xy, xg);
        $finish;
      end
      if (red + yellow + green !== 1) begin
        $display("FAIL: tick %0d outputs are not one-hot", at);
        $finish;
      end
    end
  endtask

  integer t;
  initial begin
    tick; rst = 0;
    check(1, 0, 0, 0);                     // reset: Red, timer 0
    for (t = 1; t <= 104; t = t + 1) begin
      tick;
      if (t == 1)        check(0, 0, 1, t); // Red -> Green (timer was 0); timer <- 50
      else if (t == 51)  check(0, 0, 1, t); // still Green (timer counting down)
      else if (t == 52)  check(0, 1, 0, t); // Green -> Yellow; timer <- 40
      else if (t == 92)  check(0, 1, 0, t); // still Yellow
      else if (t == 93)  check(1, 0, 0, t); // Yellow -> Red; timer <- 10
      else if (t == 103) check(1, 0, 0, t); // still Red
      else if (t == 104) check(0, 0, 1, t); // Red -> Green again: full cycle
    end
    $display("PASS");
    $finish;
  end
endmodule
