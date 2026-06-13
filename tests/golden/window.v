module Window (
    input wire [(8)-1:0] lo,
    input wire [(8)-1:0] value,
    input wire [(8)-1:0] hi,
    output wire in_range
);
    assign in_range = ((lo <= value) && (value <= hi));
endmodule

