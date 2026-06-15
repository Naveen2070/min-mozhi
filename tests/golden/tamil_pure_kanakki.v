module kannakki #(
    parameter akalam = 8
) (
    input wire katikai,
    input wire miill,
    output wire [(akalam)-1:0] kannakku
);
    reg [(akalam)-1:0] mathippu;
    assign kannakku = mathippu;
    always @(posedge katikai) begin
        if (miill) begin
            mathippu <= 0;
        end else begin
            mathippu <= (mathippu + 1);
        end
    end
endmodule

