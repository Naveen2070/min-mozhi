// Self-checking TB: nilaippatuththi — the pure-Tamil debouncer
// (examples/tamil-pure), the same circuit as Debouncer with Tamil names. Ports
// are the romanized Tamil names: clk=katikai, rst=miill, raw=muulam,
// stable=thellivu. Defaults akalam(WIDTH)=3, urruthi(STABLE)=4.
`timescale 1ns/1ps
module nilaippaduthi_tb;
  reg clk = 0, rst = 1, raw = 0;
  wire stable;
  nilaippatuththi dut (.katikai(clk), .miill(rst), .muulam(raw), .thellivu(stable));
  always #5 clk = ~clk;

  integer i;
  initial begin
    @(posedge clk); #1; // sync reset applied on this edge
    rst = 0;
    if (stable !== 1'b0) begin
      $display("FAIL: after reset stable=%b, expected 0", stable);
      $finish;
    end

    raw = 1; // sustained press is accepted
    for (i = 0; i < 10; i = i + 1) begin @(posedge clk); #1; end
    if (stable !== 1'b1) begin
      $display("FAIL: sustained press stable=%b, expected 1", stable);
      $finish;
    end

    raw = 0; // release settles back to 0
    for (i = 0; i < 10; i = i + 1) begin @(posedge clk); #1; end
    if (stable !== 1'b0) begin
      $display("FAIL: release stable=%b, expected 0", stable);
      $finish;
    end

    raw = 1; // glitch shorter than STABLE is rejected
    @(posedge clk); #1;
    @(posedge clk); #1;
    raw = 0;
    for (i = 0; i < 10; i = i + 1) begin @(posedge clk); #1; end
    if (stable !== 1'b0) begin
      $display("FAIL: short glitch stable=%b, expected 0", stable);
      $finish;
    end

    $display("PASS");
    $finish;
  end
endmodule
