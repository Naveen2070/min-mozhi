// Self-checking TB: kuutti — the pure-Tamil adder (examples/tamil-pure), the
// same circuit as Adder with Tamil names (அ=a, ஆ=aa, கூட்டு=kuuttu). Lossless
// `+`: the 9th bit carries, never drops.
`timescale 1ns/1ps
module kuutti_tb;
  reg [7:0] a, aa;
  wire [8:0] kuuttu;
  kuutti dut (.a(a), .aa(aa), .kuuttu(kuuttu));

  task check(input [7:0] xa, input [7:0] xb);
    begin
      a = xa; aa = xb; #1;
      if (kuuttu !== {1'b0, xa} + {1'b0, xb}) begin
        $display("FAIL: %0d + %0d = %0d (expected %0d)", xa, xb, kuuttu, xa + xb);
        $finish;
      end
    end
  endtask

  integer i;
  initial begin
    check(8'd0, 8'd0);
    check(8'd255, 8'd1);   // the classic dropped carry: needs bit 8
    check(8'd255, 8'd255);
    check(8'd100, 8'd55);
    for (i = 0; i < 50; i = i + 1) check(i * 37, i * 91);
    $display("PASS");
    $finish;
  end
endmodule
