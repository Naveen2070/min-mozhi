// Self-checking TB: Mac — proves the `function automatic` combinator works.
// mac(a, b) = a * b (lossless 8×8→16 multiply). The large cases (a=200,b=200
// → 40000; a=255,b=255 → 65025) show the 16-bit result holds the full product
// with no truncation — the opposite of the fn_mac_local wrap-at-8 case.
`timescale 1ns/1ps
module fn_mac_tb;
  reg [7:0] a, b;
  wire [15:0] result;
  Mac dut (.a(a), .b(b), .result(result));

  task check(input [7:0] xa, input [7:0] xb, input [15:0] xresult);
    begin
      a = xa; b = xb; #1;
      if (result !== xresult) begin
        $display("FAIL: mac(%0d, %0d) -> %0d, expected %0d",
                 xa, xb, result, xresult);
        $finish;
      end
    end
  endtask

  initial begin
    check(8'd0,   8'd0,   16'd0);
    check(8'd3,   8'd7,   16'd21);
    check(8'd10,  8'd20,  16'd200);
    check(8'd255, 8'd1,   16'd255);
    // lossless: 200*200=40000 fits in 16 bits (65535 max), no wrap
    check(8'd200, 8'd200, 16'd40000);
    // maximum product: 255*255=65025 < 65536 — still no wrap
    check(8'd255, 8'd255, 16'd65025);
    $display("PASS");
    $finish;
  end
endmodule
