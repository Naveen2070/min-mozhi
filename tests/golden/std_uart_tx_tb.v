module uart_idles_high_tb;
  reg clk;
  reg rst;
  reg start;
  reg [8-1:0] data;
  wire tx;
  wire busy;

  UartTx  #(.CLKS_PER_BIT(2)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .start(start),
    .data(data),
    .tx(tx),
    .busy(busy)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("uart_idles_high_tb.vcd");
    $dumpvars(0, uart_idles_high_tb);
    rst = 0;
    start = 0;
    data = 0;
    if (!((tx == 1))) begin
      $display("FAIL: expect %0s failed", "(tx == 1)");
      $finish;
    end
    if (!((busy == 0))) begin
      $display("FAIL: expect %0s failed", "(busy == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module uart_begins_a_frame_on_start_tb;
  reg clk;
  reg rst;
  reg start;
  reg [8-1:0] data;
  wire tx;
  wire busy;

  UartTx  #(.CLKS_PER_BIT(2)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .start(start),
    .data(data),
    .tx(tx),
    .busy(busy)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("uart_begins_a_frame_on_start_tb.vcd");
    $dumpvars(0, uart_begins_a_frame_on_start_tb);
    rst = 0;
    start = 0;
    data = 0;
    start = 1;
    data = 'h55;
    repeat (1) @(posedge clk);
    if (!((busy == 1))) begin
      $display("FAIL: expect %0s failed", "(busy == 1)");
      $finish;
    end
    if (!((tx == 0))) begin
      $display("FAIL: expect %0s failed", "(tx == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module uart_returns_to_idle_after_a_frame_tb;
  reg clk;
  reg rst;
  reg start;
  reg [8-1:0] data;
  wire tx;
  wire busy;

  UartTx  #(.CLKS_PER_BIT(2)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .start(start),
    .data(data),
    .tx(tx),
    .busy(busy)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("uart_returns_to_idle_after_a_frame_tb.vcd");
    $dumpvars(0, uart_returns_to_idle_after_a_frame_tb);
    rst = 0;
    start = 0;
    data = 0;
    start = 1;
    data = 'h55;
    repeat (1) @(posedge clk);
    start = 0;
    repeat (24) @(posedge clk);
    if (!((busy == 0))) begin
      $display("FAIL: expect %0s failed", "(busy == 0)");
      $finish;
    end
    if (!((tx == 1))) begin
      $display("FAIL: expect %0s failed", "(tx == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

