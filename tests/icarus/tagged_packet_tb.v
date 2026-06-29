// Self-checking TB: PacketDecoder — proves tagged union packet decoding works.
// Bit layout: [64] = tag (0=Read, 1=Write), [63:32] = addr, [31:0] = data.
// Read: tag=0 sets data to 0 regardless of bus input.
// Write: tag=1 uses data from bus[31:0].
`timescale 1ns/1ps
module tagged_packet_tb;
  reg [64:0] bus;
  wire is_read;
  wire [31:0] address;
  wire [31:0] wdata;
  PacketDecoder dut (.bus(bus), .is_read(is_read), .address(address), .wdata(wdata));

  task check(input [64:0] xbus, input xis_read, input [31:0] xaddr, input [31:0] xwdata);
    begin
      bus = xbus; #1;
      if (is_read !== xis_read) begin
        $display("FAIL: bus=%b, is_read=%b, expected %b", xbus, is_read, xis_read);
        $finish;
      end
      if (address !== xaddr) begin
        $display("FAIL: bus=%b, address=%h, expected %h", xbus, address, xaddr);
        $finish;
      end
      if (wdata !== xwdata) begin
        $display("FAIL: bus=%b, wdata=%h, expected %h", xbus, wdata, xwdata);
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
