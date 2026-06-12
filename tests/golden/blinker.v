module Blinker #(
    parameter LIMIT = 50000000
) (
    input wire clk,
    input wire rst,
    output wire led
);
    reg [(26)-1:0] cnt;
    reg state;
    assign led = state;
    always @(posedge clk) begin
        if (rst) begin
            cnt <= 0;
            state <= 0;
        end else begin
            if ((cnt == LIMIT)) begin
                cnt <= 0;
                state <= (state ^ 1);
            end else begin
                cnt <= (cnt + 1);
            end
        end
    end
endmodule

