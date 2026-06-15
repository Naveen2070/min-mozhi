module simitti #(
    parameter varampu = 50000000
) (
    input wire katikai,
    input wire miill,
    output wire olli
);
    reg [(26)-1:0] kannakku;
    reg nilaimai;
    assign olli = nilaimai;
    always @(posedge katikai) begin
        if (miill) begin
            kannakku <= 0;
            nilaimai <= 0;
        end else begin
            if ((kannakku == varampu)) begin
                kannakku <= 0;
                nilaimai <= (nilaimai ^ 1);
            end else begin
                kannakku <= (kannakku + 1);
            end
        end
    end
endmodule

