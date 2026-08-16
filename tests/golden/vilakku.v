module villakku (
    input wire manni,
    input wire miill,
    output wire olli
);
    reg sutar;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial sutar = 0;
    assign olli = sutar;
    always @(posedge manni) begin
        if (miill) begin
            sutar <= 0;
        end else begin
            sutar <= (!sutar);
        end
    end
endmodule

