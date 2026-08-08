module ShiftRegister #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    input wire din,
    output wire [(WIDTH)-1:0] dout
);
    reg [(WIDTH)-1:0] sr;
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

