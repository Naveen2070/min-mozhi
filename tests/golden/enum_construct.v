module EnumConstruct (
    input wire [(4)-1:0] k,
    output wire [(4)-1:0] k_out
);
    wire [8:0] p;
    assign p = {1'd0, k, 4'd0};
    assign k_out = (((p[8:8] == 1'd0)) ? ((p[7:4])) : (0));
endmodule

