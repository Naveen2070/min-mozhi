module ForeachFill (
    output wire [(32)-1:0] lamps
);
    assign lamps[7:0] = (0 * 2);
    assign lamps[15:8] = (1 * 2);
    assign lamps[23:16] = (2 * 2);
    assign lamps[31:24] = (3 * 2);
endmodule

