// Self-checking TB: Alu (match-as-mux, wrapping ops) and Top (Adder
// instance with the auto-wired 9-bit output).
`timescale 1ns/1ps
module alu_tb;
  reg [7:0] a, b;
  reg [1:0] op;
  wire [7:0] y;
  Alu dut (.a(a), .b(b), .op(op), .y(y));

  reg clk = 0, rst = 0;
  reg [7:0] x2, y2;
  wire [8:0] total;
  Top top (.clk(clk), .rst(rst), .x(x2), .y(y2), .total(total));

  task check(input [1:0] xop, input [7:0] xa, input [7:0] xb, input [7:0] want);
    begin
      op = xop; a = xa; b = xb; #1;
      if (y !== want) begin
        $display("FAIL: op=%0d a=%0d b=%0d y=%0d, expected %0d", xop, xa, xb, y, want);
        $finish;
      end
    end
  endtask

  initial begin
    check(2'b00, 8'd200, 8'd100, 8'd44);  // +% wraps: 300 - 256
    check(2'b00, 8'd3, 8'd4, 8'd7);
    check(2'b01, 8'd50, 8'd100, 8'd206);  // -% wraps: -50 + 256
    check(2'b01, 8'd9, 8'd4, 8'd5);
    check(2'b10, 8'hF0, 8'h3C, 8'h30);    // &
    check(2'b11, 8'hF0, 8'h3C, 8'hFC);    // |
    x2 = 8'd255; y2 = 8'd255; #1;
    if (total !== 9'd510) begin
      $display("FAIL: Top total=%0d, expected 510", total);
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule
