module ShiftRegister #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    input wire din,
    output wire [(WIDTH)-1:0] dout
);
    reg [(WIDTH)-1:0] sr;
    assign dout = sr;
    always @(posedge clk) begin
        if (rst) begin
            sr <= 0;
        end else begin
            sr <= ((sr << 1) | (din));
        end
    end
endmodule

