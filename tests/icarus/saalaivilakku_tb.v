// Self-checking TB: saalaivillakku — the pure-Tamil traffic light
// (examples/tamil-pure), the same FSM as TrafficLight with Tamil names.
// sivappu=red, manjsall=yellow, passai=green; katikai=clk, miill=rst.
// The timer arm is matched on the OLD state (non-blocking `<-`), so leaving
// நிறுத்து/Red loads 50, leaving செல்/Green loads 40, leaving எச்சரி/Yellow
// loads 10 — the checkpoints below encode that.
`timescale 1ns/1ps
module saalaivilakku_tb;
  reg clk = 0, rst = 1;
  wire sivappu, manjsall, passai;
  saalaivillakku dut (.katikai(clk), .miill(rst), .sivappu(sivappu), .manjsall(manjsall), .passai(passai));
  always #5 clk = ~clk;

  task tick;
    begin
      @(posedge clk); #1;
    end
  endtask

  task check(input xr, input xy, input xg, input [31:0] at);
    begin
      if (sivappu !== xr || manjsall !== xy || passai !== xg) begin
        $display("FAIL: tick %0d r/y/g=%b%b%b, expected %b%b%b", at, sivappu, manjsall, passai, xr, xy, xg);
        $finish;
      end
      if (sivappu + manjsall + passai !== 1) begin
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
      else if (t == 51)  check(0, 0, 1, t); // still Green
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
