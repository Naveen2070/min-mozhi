// Self-checking TB: perukki — the pure-Tamil multiplier (examples/tamil-pure),
// the same circuit as Mac with Tamil names (அ=a, ஆ=aa, விடை=vitai).
// Lossless `*`: 8×8→16, the full product is never truncated.
`timescale 1ns/1ps
module perukki_tb;
  reg [7:0] a, aa;
  wire [15:0] vitai;
  perukki dut (.a(a), .aa(aa), .vitai(vitai));

  task check(input [7:0] xa, input [7:0] xb, input [15:0] xresult);
    begin
      a = xa; aa = xb; #1;
      if (vitai !== xresult) begin
        $display("FAIL: mac(%0d, %0d) -> %0d, expected %0d",
                 xa, xb, vitai, xresult);
        $finish;
      end
    end
  endtask

  initial begin
    check(8'd0,   8'd0,   16'd0);
    check(8'd3,   8'd7,   16'd21);
    check(8'd10,  8'd20,  16'd200);
    check(8'd255, 8'd1,   16'd255);
    // lossless: 200*200=40000 fits in 16 bits (65535 max)
    check(8'd200, 8'd200, 16'd40000);
    // maximum product: 255*255=65025 < 65536
    check(8'd255, 8'd255, 16'd65025);
    $display("PASS");
    $finish;
  end
endmodule
