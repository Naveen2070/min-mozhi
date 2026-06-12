module Counter #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    output wire [(WIDTH)-1:0] count
);
    reg [(WIDTH)-1:0] value;
    assign count = value;
    always @(posedge clk) begin
        if (rst) begin
            value <= 0;
        end else begin
            value <= (value + 1);
        end
    end
endmodule

