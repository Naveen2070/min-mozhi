module PulseGen (
    input wire clk,
    input wire rst,
    input wire start,
    output wire done
);
    reg done_r;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial done_r = 0;
    assign done = done_r;
    always @(posedge clk) begin
        if (rst) begin
            done_r <= 0;
        end else begin
            done_r <= 0;
            if (start) begin
                done_r <= 1;
            end
        end
    end
endmodule

