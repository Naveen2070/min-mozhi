module theervi #(
    parameter akalam = 8
) (
    input wire [(2)-1:0] theervu,
    input wire [(akalam)-1:0] a,
    input wire [(akalam)-1:0] aa,
    input wire [(akalam)-1:0] i,
    input wire [(akalam)-1:0] ii,
    output wire [(akalam)-1:0] villaivu
);
    assign villaivu = (((theervu == 'b00)) ? (a) : (((theervu == 'b01)) ? (aa) : (((theervu == 'b10)) ? (i) : (ii))));
endmodule

