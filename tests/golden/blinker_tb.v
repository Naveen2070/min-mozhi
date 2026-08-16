module blinker_emulation_tb;
  reg clk;
  reg rst;
  wire led;

  Blinker  #(.LIMIT(1000000)) _dut_inst (
    .clk(clk),
    .rst(rst),
    .led(led)
  );

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    $dumpfile("blinker_emulation_tb.vcd");
    $dumpvars(0, blinker_emulation_tb);
    rst = 0;
    #1;
    rst <= 1;
    #1;
    repeat (1) @(posedge clk);
    #1;
    rst <= 0;
    #1;
    repeat (1000000) @(posedge clk);
    #1;
    $display("PASS");
    $finish;
  end
endmodule

