// Self-checking TB: BitOps — the arithmetic / reduction built-ins.
// min/max (unsigned), abs (signed, grows one bit so abs(MIN) is exact), and the
// negated reductions nand/nor/xnor. The abs(MIN) case (s = -8 → mag = +8) is the
// one that proves the result really is signed[5], not a 4-bit wrap.
`timescale 1ns/1ps
module bitops_tb;
  reg [3:0] a, b;
  reg signed [3:0] s;
  wire [3:0] lo, hi;
  wire signed [4:0] mag;
  wire nd, nr, xn;
  BitOps dut (.a(a), .b(b), .s(s), .lo(lo), .hi(hi), .mag(mag),
              .nd(nd), .nr(nr), .xn(xn));

  task check(input [3:0] xa, input [3:0] xb, input signed [3:0] xs,
             input [3:0] xlo, input [3:0] xhi, input signed [4:0] xmag,
             input xnd, input xnr, input xxn);
    begin
      a = xa; b = xb; s = xs; #1;
      if (lo !== xlo || hi !== xhi || mag !== xmag
          || nd !== xnd || nr !== xnr || xn !== xxn) begin
        $display("FAIL: a=%0d b=%0d s=%0d -> lo=%0d hi=%0d mag=%0d nd=%b nr=%b xn=%b",
                 xa, xb, xs, lo, hi, mag, nd, nr, xn);
        $finish;
      end
    end
  endtask

  initial begin
    // a=0011 b=1010 s=-3: min=3 max=10 abs=3 nand=1 nor=0 xnor(2 ones,even)=1
    check(4'd3, 4'd10, -4'sd3, 4'd3, 4'd10, 5'sd3, 1'b1, 1'b0, 1'b1);
    // a=1111 b=0000 s=-8(MIN): min=0 max=15 abs=8 nand=0 nor=0 xnor(4,even)=1
    check(4'd15, 4'd0, -4'sd8, 4'd0, 4'd15, 5'sd8, 1'b0, 1'b0, 1'b1);
    // a=0001 b=0001 s=7: min=1 max=1 abs=7 nand=1 nor=0 xnor(1 one,odd)=0
    check(4'd1, 4'd1, 4'sd7, 4'd1, 4'd1, 5'sd7, 1'b1, 1'b0, 1'b0);
    // a=0000 b=0101 s=0: min=0 max=5 abs=0 nand=1 nor(none set)=1 xnor(0,even)=1
    check(4'd0, 4'd5, 4'sd0, 4'd0, 4'd5, 5'sd0, 1'b1, 1'b1, 1'b1);
    $display("PASS");
    $finish;
  end
endmodule
