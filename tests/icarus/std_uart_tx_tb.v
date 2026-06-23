// Self-checking TB: UartTx — an 8-N-1 UART transmitter. At CLKS_PER_BIT=2 each
// bit is two clocks. Drives one frame for 0x4B and reconstructs the line: start
// bit (0), eight data bits LSB-first, stop bit (1); checks busy rises during
// the frame and falls after. 0x4B = 0100_1011 is asymmetric, so a bit-order or
// MSB/LSB mistake is caught.
`timescale 1ns/1ps
module std_uart_tx_tb;
  reg clk = 0, rst = 1, start = 0;
  reg [7:0] data = 0;
  wire tx, busy;
  UartTx #(.CLKS_PER_BIT(2)) dut (.clk(clk), .rst(rst), .start(start),
                                  .data(data), .tx(tx), .busy(busy));
  always #5 clk = ~clk;

  reg [7:0] expected;
  reg [9:0] captured;   // [0]=start, [1..8]=data LSB-first, [9]=stop
  integer b;
  initial begin
    expected = 8'h4B;
    @(posedge clk); #1; rst = 0;
    if (tx !== 1'b1 || busy !== 1'b0) begin
      $display("FAIL: idle line tx=%b busy=%b, expected 1/0", tx, busy); $finish;
    end

    // Pulse start for one clock; this edge moves Idle -> Start.
    start = 1; data = expected;
    @(posedge clk); #1;
    start = 0;
    if (busy !== 1'b1) begin
      $display("FAIL: busy did not rise at frame start"); $finish;
    end

    // Sample one bit per period (CLKS_PER_BIT=2 clocks), 10 bits total.
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

    // The frame is done — the line is idle again.
    if (busy !== 1'b0 || tx !== 1'b1) begin
      $display("FAIL: after frame busy=%b tx=%b, expected 0/1", busy, tx); $finish;
    end

    $display("PASS");
    $finish;
  end
endmodule
