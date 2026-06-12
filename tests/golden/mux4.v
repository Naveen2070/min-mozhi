module Mux4 #(
    parameter WIDTH = 8
) (
    input wire [(2)-1:0] sel,
    input wire [(WIDTH)-1:0] a,
    input wire [(WIDTH)-1:0] b,
    input wire [(WIDTH)-1:0] c,
    input wire [(WIDTH)-1:0] d,
    output wire [(WIDTH)-1:0] y
);
    assign y = (((sel == 'b00)) ? (a) : (((sel == 'b01)) ? (b) : (((sel == 'b10)) ? (c) : (d))));
endmodule

