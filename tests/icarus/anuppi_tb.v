// Self-checking TB: anuppi — the pure-Tamil UART transmitter
// (examples/tamil-pure), the same circuit as UartTx with Tamil names. Ports are
// the romanized Tamil names: clk=katikai, rst=miill, start=thotangku,
// data=tharavu, tx=vari, busy=veelai; the CLKS_PER_BIT param is `kannakku`.
`timescale 1ns/1ps
module anuppi_tb;
  reg clk = 0, rst = 1, start = 0;
  reg [7:0] data = 0;
  wire tx, busy;
  anuppi #(.kannakku(2)) dut (.katikai(clk), .miill(rst), .thotangku(start),
                              .tharavu(data), .vari(tx), .veelai(busy));
  always #5 clk = ~clk;

  reg [7:0] expected;
  reg [9:0] captured;
  integer b;
  initial begin
    expected = 8'h4B;
    @(posedge clk); #1; rst = 0;
    if (tx !== 1'b1 || busy !== 1'b0) begin
      $display("FAIL: idle line tx=%b busy=%b, expected 1/0", tx, busy); $finish;
    end

    start = 1; data = expected;
    @(posedge clk); #1;
    start = 0;
    if (busy !== 1'b1) begin
      $display("FAIL: busy did not rise at frame start"); $finish;
    end

    for (b = 0; b < 10; b = b + 1) begin
      captured[b] = tx;
      @(posedge clk); #1;
      @(posedge clk); #1;
    end

    if (captured[0] !== 1'b0) begin
      $display("FAIL: start bit=%b, expected 0", captured[0]); $finish;
    end
    for (b = 0; b < 8; b = b + 1) begin
      if (captured[b+1] !== expected[b]) begin
        $display("FAIL: data bit %0d=%b, expected %b", b, captured[b+1], expected[b]);
        $finish;
      end
    end
    if (captured[9] !== 1'b1) begin
      $display("FAIL: stop bit=%b, expected 1", captured[9]); $finish;
    end

    if (busy !== 1'b0 || tx !== 1'b1) begin
      $display("FAIL: after frame busy=%b tx=%b, expected 0/1", busy, tx); $finish;
    end

    $display("PASS");
    $finish;
  end
endmodule
