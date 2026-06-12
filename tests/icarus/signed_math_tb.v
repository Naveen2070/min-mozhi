// Self-checking TB: SignedMath — proves the emitted Verilog gets
// two's-complement semantics right. Exhaustive over all 256 (a, b)
// pairs of signed[4]:
//   ext = extend(a, 8)  must SIGN-extend (-1 stays -1, not 15),
//   sum = a + b         in 5 bits never overflows (lossless `+`),
//   lt  = a < b         must be the SIGNED comparison (-8 < 7).
`timescale 1ns/1ps
module signed_math_tb;
  reg  signed [3:0] a, b;
  wire signed [7:0] ext;
  wire signed [4:0] sum;
  wire              lt;
  SignedMath dut (.a(a), .b(b), .ext(ext), .sum(sum), .lt(lt));

  integer i, j;
  initial begin
    for (i = -8; i <= 7; i = i + 1) begin
      for (j = -8; j <= 7; j = j + 1) begin
        a = i; b = j; #1;
        if (ext !== i) begin
          $display("FAIL: extend(%0d, 8) -> %0d (sign lost?)", i, ext);
          $finish;
        end
        if (sum !== i + j) begin
          $display("FAIL: %0d + %0d -> %0d", i, j, sum);
          $finish;
        end
        if (lt !== (i < j)) begin
          $display("FAIL: (%0d < %0d) -> %b (unsigned compare?)", i, j, lt);
          $finish;
        end
      end
    end
    $display("PASS");
    $finish;
  end
endmodule
