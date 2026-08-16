module Counter #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    output wire [(WIDTH)-1:0] count
);
    reg [(WIDTH)-1:0] value;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial value = 0;
    assign count = value;
    always @(posedge clk) begin
        if (rst) begin
            value <= 0;
        end else begin
            value <= (value + 8'd1);
        end
    end
endmodule

