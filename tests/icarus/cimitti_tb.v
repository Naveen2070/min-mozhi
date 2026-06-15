// Self-checking TB: simitti — the pure-Tamil blinker (examples/tamil-pure), the
// same circuit as Blinker with Tamil names. varampu (LIMIT) is a parameter, so
// the TB instantiates #(.varampu(3)) and sees the toggle in 4 ticks instead of
// 50 million cycles. Ports: clk=katikai, rst=miill, led=olli.
`timescale 1ns/1ps
module cimitti_tb;
  reg clk = 0, rst = 1;
  wire led;
  simitti #(.varampu(3)) dut (.katikai(clk), .miill(rst), .olli(led));
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
    tick; check(1'b1, 4);   // cnt hit varampu -> state toggles
    for (i = 5; i <= 7; i = i + 1) begin
      tick; check(1'b1, i);
    end
    tick; check(1'b0, 8);   // and toggles back
    $display("PASS");
    $finish;
  end
endmodule
