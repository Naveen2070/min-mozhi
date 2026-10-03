module Mux4 (
    input wire [1:0] sel,
    input wire [7:0] a,
    input wire [7:0] b,
    input wire [7:0] c,
    input wire [7:0] d,
    output wire [7:0] y
);
    wire [1:0] c0_const;
    wire c1_eq;
    wire [7:0] c2_mux;
    wire [1:0] c3_const;
    wire c4_eq;
    wire [7:0] c5_mux;
    wire [1:0] c6_const;
    wire c7_eq;
    wire [7:0] c8_mux;
    assign c0_const = 2'd2;
    assign c1_eq = sel == c0_const;
    assign c2_mux = c1_eq ? c : d;
    assign c3_const = 2'd1;
    assign c4_eq = sel == c3_const;
    assign c5_mux = c4_eq ? b : c2_mux;
    assign c6_const = 2'd0;
    assign c7_eq = sel == c6_const;
    assign c8_mux = c7_eq ? a : c5_mux;
    assign y = c8_mux;
endmodule
