module fifo_starts_empty_tb;
  reg clk;
  reg rst;
  reg push;
  reg pop;
  reg [8-1:0] din;
  wire full;
  wire empty;
  wire [8-1:0] dout;

  Fifo  #(.WIDTH(8), .DEPTH(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .push(push),
    .pop(pop),
    .din(din),
    .full(full),
    .empty(empty),
    .dout(dout)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("fifo_starts_empty_tb.vcd");
    $dumpvars(0, fifo_starts_empty_tb);
    rst = 0;
    push = 0;
    pop = 0;
    din = 0;
    if (!((empty == 1))) begin
      $display("FAIL: expect %0s failed", "(empty == 1)");
      $finish;
    end
    if (!((full == 0))) begin
      $display("FAIL: expect %0s failed", "(full == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module fifo_round_trips_a_byte_tb;
  reg clk;
  reg rst;
  reg push;
  reg pop;
  reg [8-1:0] din;
  wire full;
  wire empty;
  wire [8-1:0] dout;

  Fifo  #(.WIDTH(8), .DEPTH(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .push(push),
    .pop(pop),
    .din(din),
    .full(full),
    .empty(empty),
    .dout(dout)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("fifo_round_trips_a_byte_tb.vcd");
    $dumpvars(0, fifo_round_trips_a_byte_tb);
    rst = 0;
    push = 0;
    pop = 0;
    din = 0;
    push = 1;
    din = 'hA5;
    repeat (1) @(posedge clk);
    push = 0;
    if (!((dout == 'hA5))) begin
      $display("FAIL: expect %0s failed", "(dout == 'hA5)");
      $finish;
    end
    if (!((empty == 0))) begin
      $display("FAIL: expect %0s failed", "(empty == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module fifo_fills_up_tb;
  reg clk;
  reg rst;
  reg push;
  reg pop;
  reg [8-1:0] din;
  wire full;
  wire empty;
  wire [8-1:0] dout;

  Fifo  #(.WIDTH(8), .DEPTH(4)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .push(push),
    .pop(pop),
    .din(din),
    .full(full),
    .empty(empty),
    .dout(dout)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("fifo_fills_up_tb.vcd");
    $dumpvars(0, fifo_fills_up_tb);
    rst = 0;
    push = 0;
    pop = 0;
    din = 0;
    push = 1;
    din = 1;
    repeat (4) @(posedge clk);
    if (!((full == 1))) begin
      $display("FAIL: expect %0s failed", "(full == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

