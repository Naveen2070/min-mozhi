// Self-checking TB: kannakki — the pure-Tamil counter (examples/tamil-pure),
// the same circuit as Counter with Tamil names. Sync reset to 0, +1 per rising
// edge, `+%` wraps 255 -> 0 (the loop runs past 256 on purpose). Ports are the
// romanized Tamil names: clk=katikai, rst=miill, count=kannakku.
`timescale 1ns/1ps
module kanakki_tb;
  reg clk = 0, rst = 1;
  wire [7:0] count;
  kannakki dut (.katikai(clk), .miill(rst), .kannakku(count));
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
