module Adder (
    input wire [7:0] a,
    input wire [7:0] b,
    output wire [8:0] sum
);
    wire [8:0] c0_add;
    assign c0_add = {1'b0, a} + {1'b0, b};
    assign sum = c0_add;
endmodule
