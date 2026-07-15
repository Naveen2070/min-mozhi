module Pll #(parameter MULT = 2) (
    input  wire clk_in,
    output wire clk_out,
    output wire locked
);
    assign clk_out = clk_in;
    assign locked = 1'b1;
endmodule
