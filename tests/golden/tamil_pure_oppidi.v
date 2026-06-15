module oppiti #(
    parameter akalam = 8
) (
    input wire [(akalam)-1:0] a,
    input wire [(akalam)-1:0] aa,
    output wire samam,
    output wire perithu,
    output wire [(akalam)-1:0] periyathu
);
    assign samam = (a == aa);
    assign perithu = (a > aa);
    assign periyathu = (((a > aa)) ? (a) : (aa));
endmodule

