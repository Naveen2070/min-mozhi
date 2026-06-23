module minukki #(
    parameter akalam = 8
) (
    input wire katikai,
    input wire miill,
    input wire [(akalam)-1:0] katamai,
    output wire alai
);
    reg [(akalam)-1:0] ennnni;
    assign alai = (ennnni < katamai);
    always @(posedge katikai) begin
        if (miill) begin
            ennnni <= 0;
        end else begin
            ennnni <= (ennnni + 1);
        end
    end
endmodule

