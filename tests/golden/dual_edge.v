module DualEdge (
    input wire clk,
    input wire rst,
    input wire [(8)-1:0] d,
    output wire [(8)-1:0] q
);
    reg [(8)-1:0] a;
    reg [(8)-1:0] b;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial a = 0;
    initial b = 0;
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

