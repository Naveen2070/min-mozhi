module ShiftRegister #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    input wire din,
    output wire [(WIDTH)-1:0] dout
);
    reg [(WIDTH)-1:0] sr;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 sr = 0;
    wire [8:0] __mimz_sub_1;
    assign __mimz_sub_1 = (sr << 1);
    assign dout = sr;
    always @(posedge clk) begin
        if (rst) begin
            sr <= 0;
        end else begin
            sr <= (__mimz_sub_1[(WIDTH)-1:0] | (din));
        end
    end
endmodule

