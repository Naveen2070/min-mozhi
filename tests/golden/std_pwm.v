module Pwm #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    input wire [(WIDTH)-1:0] duty,
    output wire pwm
);
    reg [(WIDTH)-1:0] counter;
    assign pwm = (counter < duty);
    always @(posedge clk) begin
        if (rst) begin
            counter <= 0;
        end else begin
            counter <= (counter + 1);
        end
    end
endmodule

