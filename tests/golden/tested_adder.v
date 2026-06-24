module Adder #(
    parameter WIDTH = 8
) (
    input wire [(WIDTH)-1:0] a,
    input wire [(WIDTH)-1:0] b,
    output wire [((WIDTH + 1))-1:0] sum
);
    assign sum = (a + b);
endmodule

