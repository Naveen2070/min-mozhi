module Alu #(
    parameter WIDTH = 8
) (
    input wire [(WIDTH)-1:0] a,
    input wire [(WIDTH)-1:0] b,
    input wire [(2)-1:0] op,
    output wire [(WIDTH)-1:0] y
);
    assign y = (((op == 'b00)) ? ((a + b)) : (((op == 'b01)) ? ((a - b)) : (((op == 'b10)) ? ((a & b)) : ((a | b)))));
endmodule

module Top (
    input wire clk,
    input wire rst,
    input wire [(8)-1:0] x,
    input wire [(8)-1:0] y,
    output wire [(9)-1:0] total
);
    wire [((8 + 1))-1:0] add_sum;
    Adder #(.WIDTH(8)) add (.a(x), .b(y), .sum(add_sum));
    assign total = add_sum;
endmodule

module Adder #(
    parameter WIDTH = 8
) (
    input wire [(WIDTH)-1:0] a,
    input wire [(WIDTH)-1:0] b,
    output wire [((WIDTH + 1))-1:0] sum
);
    assign sum = (a + b);
endmodule

