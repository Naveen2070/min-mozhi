module SignedMath (
    input wire signed [(4)-1:0] a,
    input wire signed [(4)-1:0] b,
    output wire signed [(8)-1:0] ext,
    output wire signed [(5)-1:0] sum,
    output wire lt
);
    assign ext = (a);
    assign sum = (a + b);
    assign lt = (a < b);
endmodule

