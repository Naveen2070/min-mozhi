module traffic_light_emulation_tb;
  reg clk;
  reg rst;
  wire red;
  wire yellow;
  wire green;

  TrafficLight _dut_inst (
    .clk(clk),
    .rst(rst),
    .red(red),
    .yellow(yellow),
    .green(green)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("traffic_light_emulation_tb.vcd");
    $dumpvars(0, traffic_light_emulation_tb);
    rst = 0;
    #1;
    rst <= 1;
    #1;
    repeat (1) @(posedge clk);
    #1;
    rst <= 0;
    #1;
    repeat (200) @(posedge clk);
    #1;
    $display("PASS");
    $finish;
  end
endmodule

