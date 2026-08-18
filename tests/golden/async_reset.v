module ACounter (
    input wire clk,
    input wire rst,
    output wire [(8)-1:0] count
);
    reg [(8)-1:0] value;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 value = 0;
    assign count = value;
    always @(posedge clk or posedge rst) begin
        if (rst) begin
            value <= 0;
        end else begin
            value <= (value + 8'd1);
        end
    end
endmodule

