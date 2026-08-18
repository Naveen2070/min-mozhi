module EdgeDetector (
    input wire clk,
    input wire rst,
    input wire din,
    output wire pulse
);
    reg prev;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 prev = 0;
    assign pulse = (din && (!prev));
    always @(posedge clk) begin
        if (rst) begin
            prev <= 0;
        end else begin
            prev <= din;
        end
    end
endmodule

