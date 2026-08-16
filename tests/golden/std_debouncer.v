module Debouncer #(
    parameter WIDTH = 3,
    parameter STABLE = 4
) (
    input wire clk,
    input wire rst,
    input wire raw,
    output wire stable
);
    reg sync0;
    reg sync1;
    reg [(WIDTH)-1:0] cnt;
    reg out_q;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial sync0 = 0;
    initial sync1 = 0;
    initial cnt = 0;
    initial out_q = 0;
    assign stable = out_q;
    always @(posedge clk) begin
        if (rst) begin
            sync0 <= 0;
            sync1 <= 0;
            cnt <= 0;
            out_q <= 0;
        end else begin
            sync0 <= raw;
            sync1 <= sync0;
            if ((sync1 == out_q)) begin
                cnt <= 0;
            end else begin
                if ((cnt == STABLE)) begin
                    out_q <= sync1;
                    cnt <= 0;
                end else begin
                    cnt <= (cnt + 3'd1);
                end
            end
        end
    end
endmodule

