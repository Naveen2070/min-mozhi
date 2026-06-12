module EdgeDetector (
    input wire clk,
    input wire rst,
    input wire din,
    output wire pulse
);
    reg prev;
    assign pulse = (din && (!prev));
    always @(posedge clk) begin
        if (rst) begin
            prev <= 0;
        end else begin
            prev <= din;
        end
    end
endmodule

