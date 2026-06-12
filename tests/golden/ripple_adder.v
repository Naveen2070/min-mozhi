module RippleAdder (
    input wire [(4)-1:0] a,
    input wire [(4)-1:0] b,
    input wire cin,
    output wire [(4)-1:0] sum,
    output wire cout
);
    wire fa__0_sum;
    wire fa__0_cout;
    FullAdder fa__0 (.a(a[0]), .b(b[0]), .cin(cin), .sum(fa__0_sum), .cout(fa__0_cout));
    wire fa__1_sum;
    wire fa__1_cout;
    FullAdder fa__1 (.a(a[1]), .b(b[1]), .cin(fa__0_cout), .sum(fa__1_sum), .cout(fa__1_cout));
    wire fa__2_sum;
    wire fa__2_cout;
    FullAdder fa__2 (.a(a[2]), .b(b[2]), .cin(fa__1_cout), .sum(fa__2_sum), .cout(fa__2_cout));
    wire fa__3_sum;
    wire fa__3_cout;
    FullAdder fa__3 (.a(a[3]), .b(b[3]), .cin(fa__2_cout), .sum(fa__3_sum), .cout(fa__3_cout));
    assign sum[0] = fa__0_sum;
    assign sum[1] = fa__1_sum;
    assign sum[2] = fa__2_sum;
    assign sum[3] = fa__3_sum;
    assign cout = fa__3_cout;
endmodule

module FullAdder (
    input wire a,
    input wire b,
    input wire cin,
    output wire sum,
    output wire cout
);
    assign sum = ((a ^ b) ^ cin);
    assign cout = ((a & b) | (cin & (a ^ b)));
endmodule

