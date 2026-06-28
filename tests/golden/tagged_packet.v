module PacketDecoder (
    input wire [64:0] bus,
    output wire is_read,
    output wire [(32)-1:0] address,
    output wire [(32)-1:0] wdata
);
    assign is_read = (((bus[64:64] == 1'd0)) ? (1) : (0));
    assign address = (((bus[64:64] == 1'd0)) ? ((bus[63:32])) : ((bus[63:32])));
    assign wdata = (((bus[64:64] == 1'd0)) ? (0) : ((bus[31:0])));
endmodule

