module Shift #(
    parameter AMOUNT = 2
) (
    input wire [(4)-1:0] din,
    output wire [(8)-1:0] literal_shift,
    output wire [(8)-1:0] param_shift,
    output wire [(8)-1:0] var_shift
);
    wire [3:0] __mimz_sub_1;
    assign __mimz_sub_1 = (1 << 3);
    wire [5:0] __mimz_sub_2;
    assign __mimz_sub_2 = (din << 2);
    assign literal_shift = (__mimz_sub_1);
    assign param_shift = ((3 << AMOUNT));
    assign var_shift = (__mimz_sub_2);
endmodule

