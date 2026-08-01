// Self-checking TB: pothivaasi — the pure-Tamil twin of `tagged_packet_tb.v`
// (english/tagged_packet.mimz -> tamil-pure/sirappu_pothi.mimz). Same tagged-
// union packet decoding coverage, through the romanized identifiers:
// bus/is_read/address/wdata -> vari/patikkum/mukavari/mathippu.
// Bit layout: [64] = tag (0=Read, 1=Write), [63:32] = addr, [31:0] = data.
`timescale 1ns/1ps
module pothivaasi_tb;
  reg [64:0] vari;
  wire patikkum;
  wire [31:0] mukavari;
  wire [31:0] mathippu;
  pothivaasi dut (.vari(vari), .patikkum(patikkum), .mukavari(mukavari), .mathippu(mathippu));

  task check(input [64:0] xvari, input xpatikkum, input [31:0] xmukavari, input [31:0] xmathippu);
    begin
      vari = xvari; #1;
      if (patikkum !== xpatikkum) begin
        $display("FAIL: vari=%b, patikkum=%b, expected %b", xvari, patikkum, xpatikkum);
        $finish;
      end
      if (mukavari !== xmukavari) begin
        $display("FAIL: vari=%b, mukavari=%h, expected %h", xvari, mukavari, xmukavari);
        $finish;
      end
      if (mathippu !== xmathippu) begin
        $display("FAIL: vari=%b, mathippu=%h, expected %h", xvari, mathippu, xmathippu);
        $finish;
      end
    end
  endtask

  initial begin
    // Read packet: tag=0, addr=0xDEAD_BEEF, data field ignored (forced to 0)
    check({1'b0, 32'hDEAD_BEEF, 32'h0000_0000}, 1'b1, 32'hDEAD_BEEF, 32'h0000_0000);
    // Write packet: tag=1, addr=0xCAFE_0001, wdata=0xABCD_1234
    check({1'b1, 32'hCAFE_0001, 32'hABCD_1234}, 1'b0, 32'hCAFE_0001, 32'hABCD_1234);
    // Edge: all zeros (tag=0 = Read)
    check({1'b0, 32'h0000_0000, 32'h0000_0000}, 1'b1, 32'h0000_0000, 32'h0000_0000);
    // Edge: all ones (tag=1 = Write)
    check({1'b1, 32'hFFFF_FFFF, 32'hFFFF_FFFF}, 1'b0, 32'hFFFF_FFFF, 32'hFFFF_FFFF);
    $display("PASS");
    $finish;
  end
endmodule
