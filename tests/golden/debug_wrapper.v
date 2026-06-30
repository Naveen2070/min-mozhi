module DebugWrapper (
    input wire [(8)-1:0] data_in,
    output wire [(8)-1:0] data_out,
    output wire [(8)-1:0] dbg_out
);
    assign dbg_out = 0;
    assign data_out = data_in;
endmodule

