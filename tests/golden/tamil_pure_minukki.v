module minukki #(
    parameter akalam = 8
) (
    input wire katikai,
    input wire miill,
    input wire [(akalam)-1:0] katamai,
    output wire alai
);
    reg [(akalam)-1:0] ennnni;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial ennnni = 0;
    assign alai = (ennnni < katamai);
    always @(posedge katikai) begin
        if (miill) begin
            ennnni <= 0;
        end else begin
            ennnni <= (ennnni + 8'd1);
        end
    end
endmodule

