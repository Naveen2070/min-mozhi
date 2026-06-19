module kuutti #(
    parameter akalam = 8
) (
    input wire [(akalam)-1:0] a,
    input wire [(akalam)-1:0] aa,
    output wire [((akalam + 1))-1:0] kuuttu
);
    assign kuuttu = (a + aa);
endmodule

