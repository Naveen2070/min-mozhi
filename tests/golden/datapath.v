module DataPath (
    input wire [(4)-1:0] a,
    input wire [(4)-1:0] b,
    output wire [(8)-1:0] prod,
    output wire [(4)-1:0] wrap,
    output wire [(4)-1:0] rsh,
    output wire [(8)-1:0] cat,
    output wire [(2)-1:0] hi2,
    output wire [(2)-1:0] lo2
);
    assign prod = (a * b);
    assign wrap = (a * b);
    assign rsh = (a >> 1);
    assign cat = {a, b};
    assign hi2 = a[3:2];
    assign lo2 = a[(2)-1:0];
endmodule

