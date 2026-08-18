module Blinker #(
    parameter LIMIT = 50000000
) (
    input wire clk,
    input wire rst,
    output wire led
);
    reg [(26)-1:0] cnt;
    reg state;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 cnt = 0;
    initial #0 state = 0;
    assign led = state;
    always @(posedge clk) begin
        if (rst) begin
            cnt <= 0;
            state <= 0;
        end else begin
            if ((cnt == LIMIT)) begin
                cnt <= 0;
                state <= (state ^ 1'd1);
            end else begin
                cnt <= (cnt + 26'd1);
            end
        end
    end
endmodule

