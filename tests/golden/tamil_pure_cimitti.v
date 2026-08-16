module simitti #(
    parameter varampu = 50000000
) (
    input wire katikai,
    input wire miill,
    output wire olli
);
    reg [(26)-1:0] kannakku;
    reg nilaimai;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial kannakku = 0;
    initial nilaimai = 0;
    assign olli = nilaimai;
    always @(posedge katikai) begin
        if (miill) begin
            kannakku <= 0;
            nilaimai <= 0;
        end else begin
            if ((kannakku == varampu)) begin
                kannakku <= 0;
                nilaimai <= (nilaimai ^ 1'd1);
            end else begin
                kannakku <= (kannakku + 26'd1);
            end
        end
    end
endmodule

