module debouncer_settles_after_a_steady_input_tb;
  reg clk;
  reg rst;
  reg raw;
  wire stable;

  Debouncer  #(.WIDTH(3), .STABLE(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .raw(raw),
    .stable(stable)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("debouncer_settles_after_a_steady_input_tb.vcd");
    $dumpvars(0, debouncer_settles_after_a_steady_input_tb);
    rst = 0;
    raw = 0;
    raw = 1;
    repeat (8) @(posedge clk);
    if (!((stable == 1))) begin
      $display("FAIL: expect %0s failed", "(stable == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module debouncer_rejects_a_short_glitch_tb;
  reg clk;
  reg rst;
  reg raw;
  wire stable;

  Debouncer  #(.WIDTH(3), .STABLE(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .raw(raw),
    .stable(stable)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("debouncer_rejects_a_short_glitch_tb.vcd");
    $dumpvars(0, debouncer_rejects_a_short_glitch_tb);
    rst = 0;
    raw = 0;
    raw = 1;
    repeat (2) @(posedge clk);
    raw = 0;
    repeat (8) @(posedge clk);
    if (!((stable == 0))) begin
      $display("FAIL: expect %0s failed", "(stable == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

