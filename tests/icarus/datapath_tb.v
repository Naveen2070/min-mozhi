// Self-checking TB: DataPath — the datapath operators no other example reaches.
// Lossless `*` (prod, 8 bits) vs wrapping `*%` (wrap, 4 bits); right shift `>>`;
// concat `{a, b}` (a is the high half); slice `a[3:2]`; and `trunc(a, 2)`.
// The a=b=15 case is the proof: prod=225 keeps the full product while wrap=1 is
// the same product truncated to 4 bits — the lossless/wrapping split is real.
`timescale 1ns/1ps
module datapath_tb;
  reg [3:0] a, b;
  wire [7:0] prod, cat;
  wire [3:0] wrap, rsh;
  wire [1:0] hi2, lo2;
  DataPath dut (.a(a), .b(b), .prod(prod), .wrap(wrap), .rsh(rsh),
                .cat(cat), .hi2(hi2), .lo2(lo2));

  task check(input [3:0] xa, input [3:0] xb,
             input [7:0] xprod, input [3:0] xwrap, input [3:0] xrsh,
             input [7:0] xcat, input [1:0] xhi2, input [1:0] xlo2);
    begin
      a = xa; b = xb; #1;
      if (prod !== xprod || wrap !== xwrap || rsh !== xrsh
          || cat !== xcat || hi2 !== xhi2 || lo2 !== xlo2) begin
        $display("FAIL: a=%0d b=%0d -> prod=%0d wrap=%0d rsh=%0d cat=%0d hi2=%0d lo2=%0d",
                 xa, xb, prod, wrap, rsh, cat, hi2, lo2);
        $finish;
      end
    end
  endtask

  initial begin
    // a=0011 b=1010: prod=30 wrap=14 rsh=1 cat=00111010=58 hi2=00=0 lo2=11=3
    check(4'd3, 4'd10, 8'd30, 4'd14, 4'd1, 8'd58, 2'd0, 2'd3);
    // a=1111 b=1111: prod=225 wrap=225&15=1 rsh=7 cat=11111111=255 hi2=3 lo2=3
    check(4'd15, 4'd15, 8'd225, 4'd1, 4'd7, 8'd255, 2'd3, 2'd3);
    // a=1100 b=0101: prod=60 wrap=12 rsh=6 cat=11000101=197 hi2=11=3 lo2=00=0
    check(4'd12, 4'd5, 8'd60, 4'd12, 4'd6, 8'd197, 2'd3, 2'd0);
    // a=0000 b=0000: everything zero
    check(4'd0, 4'd0, 8'd0, 4'd0, 4'd0, 8'd0, 2'd0, 2'd0);
    $display("PASS");
    $finish;
  end
endmodule
