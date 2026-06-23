module Shift #(
    parameter AMOUNT = 2
) (
    input wire [(4)-1:0] din,
    output wire [(8)-1:0] literal_shift,
    output wire [(8)-1:0] param_shift,
    output wire [(8)-1:0] var_shift
);
    assign literal_shift = ((1 << 3));
    assign param_shift = ((3 << AMOUNT));
    assign var_shift = ((din << 2));
endmodule

