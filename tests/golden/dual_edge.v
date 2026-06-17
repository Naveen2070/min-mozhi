module DualEdge (
    input wire clk,
    input wire rst,
    input wire [(8)-1:0] d,
    output wire [(8)-1:0] q
);
    reg [(8)-1:0] a;
    reg [(8)-1:0] b;
    assign q = b;
    always @(posedge clk) begin
        if (rst) begin
            a <= 0;
        end else begin
            a <= d;
        end
    end
    always @(negedge clk) begin
        if (rst) begin
            b <= 0;
        end else begin
            b <= a;
        end
    end
endmodule

