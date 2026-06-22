module Debouncer #(
    parameter WIDTH = 3,
    parameter STABLE = 4
) (
    input wire clk,
    input wire rst,
    input wire raw,
    output wire stable
);
    reg sync0;
    reg sync1;
    reg [(WIDTH)-1:0] cnt;
    reg out_q;
    assign stable = out_q;
    always @(posedge clk) begin
        if (rst) begin
            sync0 <= 0;
            sync1 <= 0;
            cnt <= 0;
            out_q <= 0;
        end else begin
            sync0 <= raw;
            sync1 <= sync0;
            if ((sync1 == out_q)) begin
                cnt <= 0;
            end else begin
                if ((cnt == STABLE)) begin
                    out_q <= sync1;
                    cnt <= 0;
                end else begin
                    cnt <= (cnt + 1);
                end
            end
        end
    end
endmodule

