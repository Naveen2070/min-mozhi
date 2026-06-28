module Decoder (
    input wire [64:0] bus,
    output wire [(32)-1:0] raddr
);
    localparam [64:0] PACKET_READ = 65'h0;
    localparam [64:0] PACKET_WRITE = 65'h10000000000000000;
    assign raddr = (((bus[64:64] == 1'd0)) ? ((bus[63:32])) : (0));
endmodule

