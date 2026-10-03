module RegFile (
    input wire we,
    input wire [1:0] waddr,
    input wire [7:0] wdata,
    input wire [1:0] raddr,
    input wire clk,
    output wire [7:0] rdata
);
    wire c0_const;
    wire [1:0] c1_const;
    wire [7:0] c2_const;
    wire c3_const;
    wire [1:0] c4_mux;
    wire [7:0] c5_mux;
    wire c6_mux;
    wire [7:0] m;
    reg [7:0] mem7 [0:3];
    integer mem7_i;
    initial for (mem7_i = 0; mem7_i < 4; mem7_i = mem7_i + 1) mem7[mem7_i] = 8'd0;
    assign c0_const = 1'd0;
    assign c1_const = 2'd0;
    assign c2_const = 8'd0;
    assign c3_const = 1'd1;
    assign c4_mux = we ? waddr : c1_const;
    assign c5_mux = we ? wdata : c2_const;
    assign c6_mux = we ? c3_const : c0_const;
    always @(posedge clk) if (c6_mux) mem7[c4_mux] <= c5_mux;
    assign m = mem7[raddr];
    assign rdata = m;
endmodule
