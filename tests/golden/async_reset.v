module ACounter (
    input wire clk,
    input wire rst,
    output wire [(8)-1:0] count
);
    reg [(8)-1:0] value;
    assign count = value;
    always @(posedge clk or posedge rst) begin
        if (rst) begin
            value <= 0;
        end else begin
            value <= (value + 1);
        end
    end
endmodule

