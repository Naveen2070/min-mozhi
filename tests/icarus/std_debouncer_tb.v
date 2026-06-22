// Self-checking TB: Debouncer — a 2-FF synchronizer + an N-sample stability
// counter. With the defaults (WIDTH=3, STABLE=4) a steady input is accepted
// after the sync delay plus STABLE samples; a pulse shorter than STABLE is
// ignored. Sync reset clears the output to 0.
`timescale 1ns/1ps
module std_debouncer_tb;
  reg clk = 0, rst = 1, raw = 0;
  wire stable;
  Debouncer dut (.clk(clk), .rst(rst), .raw(raw), .stable(stable));
  always #5 clk = ~clk;

  integer i;
  initial begin
    @(posedge clk); #1; // sync reset applied on this edge
    rst = 0;
    if (stable !== 1'b0) begin
      $display("FAIL: after reset stable=%b, expected 0", stable);
      $finish;
    end

    // A sustained press is accepted (allow sync + STABLE samples, with margin).
    raw = 1;
    for (i = 0; i < 10; i = i + 1) begin @(posedge clk); #1; end
    if (stable !== 1'b1) begin
      $display("FAIL: sustained press stable=%b, expected 1", stable);
      $finish;
    end

    // Releasing settles back to 0.
    raw = 0;
    for (i = 0; i < 10; i = i + 1) begin @(posedge clk); #1; end
    if (stable !== 1'b0) begin
      $display("FAIL: release stable=%b, expected 0", stable);
      $finish;
    end

    // A glitch shorter than STABLE samples is rejected — output stays 0.
    raw = 1;
    @(posedge clk); #1;
    @(posedge clk); #1; // only 2 samples high
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
