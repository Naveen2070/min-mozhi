module pothivaasi (
    input wire [64:0] vari,
    output wire patikkum,
    output wire [(32)-1:0] mukavari,
    output wire [(32)-1:0] mathippu
);
    assign patikkum = (((vari[64:64] == 1'd0)) ? (1) : (0));
    assign mukavari = (((vari[64:64] == 1'd0)) ? ((vari[63:32])) : ((vari[63:32])));
    assign mathippu = (((vari[64:64] == 1'd0)) ? (0) : ((vari[31:0])));
endmodule

