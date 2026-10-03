module ACounter (
    input wire clk,
    input wire rst,
    output wire [7:0] count
);
    wire [7:0] c0_const;
    wire [7:0] c1_addwrap;
    reg [7:0] value = 0;
    assign c0_const = 8'd1;
    assign c1_addwrap = value + c0_const;
    always @(posedge clk or posedge rst)
        if (rst) value <= 8'd0;
        else value <= c1_addwrap;
    assign count = value;
endmodule
