// Self-checking TB: Seg7 — a BCD → 7-segment decoder. Decimal digits 0–9 map
// to their gfedcba glyph (active high); any other nibble (10–15) blanks the
// display. Pure combinational, so each input settles with a #1 delay.
`timescale 1ns/1ps
module std_seg7_tb;
  reg  [3:0] digit;
  wire [6:0] seg;
  Seg7 dut (.digit(digit), .seg(seg));

  // Expected glyphs for 0–9 (gfedcba, active high).
  reg [6:0] glyph [0:9];
  integer i;
  initial begin
    glyph[0] = 7'h3F; glyph[1] = 7'h06; glyph[2] = 7'h5B; glyph[3] = 7'h4F;
    glyph[4] = 7'h66; glyph[5] = 7'h6D; glyph[6] = 7'h7D; glyph[7] = 7'h07;
    glyph[8] = 7'h7F; glyph[9] = 7'h6F;

    for (i = 0; i <= 9; i = i + 1) begin
      digit = i[3:0]; #1;
      if (seg !== glyph[i]) begin
        $display("FAIL: digit %0d seg=%b, expected %b", i, seg, glyph[i]);
        $finish;
      end
    end

    // Non-decimal nibbles (10–15) blank the display.
    for (i = 10; i <= 15; i = i + 1) begin
      digit = i[3:0]; #1;
      if (seg !== 7'b0000000) begin
        $display("FAIL: digit %0d seg=%b, expected blank", i, seg);
        $finish;
      end
    end

    $display("PASS");
    $finish;
  end
endmodule
