// Self-checking TB: Mux4 — every select routes the right input.
`timescale 1ns/1ps
module mux4_tb;
  reg [1:0] sel;
  reg [7:0] a, b, c, d;
  wire [7:0] y;
  Mux4 dut (.sel(sel), .a(a), .b(b), .c(c), .d(d), .y(y));

  task check(input [1:0] s, input [7:0] want);
    begin
      sel = s; #1;
      if (y !== want) begin
        $display("FAIL: sel=%0d y=%h, expected %h", s, y, want);
        $finish;
      end
    end
  endtask

  initial begin
    a = 8'h11; b = 8'h22; c = 8'h33; d = 8'h44;
    check(2'd0, 8'h11);
    check(2'd1, 8'h22);
    check(2'd2, 8'h33);
    check(2'd3, 8'h44);
    $display("PASS");
    $finish;
  end
endmodule
