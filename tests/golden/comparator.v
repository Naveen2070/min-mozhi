module Comparator #(
    parameter WIDTH = 8
) (
    input wire [(WIDTH)-1:0] a,
    input wire [(WIDTH)-1:0] b,
    output wire eq,
    output wire gt,
    output wire [(WIDTH)-1:0] max
);
    assign eq = (a == b);
    assign gt = (a > b);
    assign max = (((a > b)) ? (a) : (b));
endmodule

