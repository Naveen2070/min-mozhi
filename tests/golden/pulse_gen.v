module PulseGen (
    input wire clk,
    input wire rst,
    input wire start,
    output wire done
);
    reg done_r;
    assign done = done_r;
    always @(posedge clk) begin
        if (rst) begin
            done_r <= 0;
        end else begin
            done_r <= 0;
            if (start) begin
                done_r <= 1;
            end
        end
    end
endmodule

