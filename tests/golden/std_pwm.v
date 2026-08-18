module Pwm #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    input wire [(WIDTH)-1:0] duty,
    output wire pwm
);
    reg [(WIDTH)-1:0] counter;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 counter = 0;
    assign pwm = (counter < duty);
    always @(posedge clk) begin
        if (rst) begin
            counter <= 0;
        end else begin
            counter <= (counter + 8'd1);
        end
    end
endmodule

