module kannakki #(
    parameter akalam = 8
) (
    input wire katikai,
    input wire miill,
    output wire [(akalam)-1:0] kannakku
);
    reg [(akalam)-1:0] mathippu;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial mathippu = 0;
    assign kannakku = mathippu;
    always @(posedge katikai) begin
        if (miill) begin
            mathippu <= 0;
        end else begin
            mathippu <= (mathippu + 8'd1);
        end
    end
endmodule

