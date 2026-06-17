module RegFile (
    input wire clk,
    input wire we,
    input wire [(2)-1:0] waddr,
    input wire [(8)-1:0] wdata,
    input wire [(2)-1:0] raddr,
    output wire [(8)-1:0] rdata
);
    reg [(8)-1:0] m [0:(4)-1];
    integer __mimz_m_i;
    initial for (__mimz_m_i = 0; __mimz_m_i < (4); __mimz_m_i = __mimz_m_i + 1) m[__mimz_m_i] = 0;
    assign rdata = m[raddr];
    always @(posedge clk) begin
        if (we) begin
            m[waddr] <= wdata;
        end
    end
endmodule

