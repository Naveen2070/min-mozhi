module Chained (
    input wire a0,
    input wire a1,
    input wire b0,
    input wire b1,
    input wire cin,
    output wire sum0,
    output wire sum1,
    output wire cout
);
    wire fa0_sum;
    wire fa0_cout;
    FullAdder fa0 (.a(a0), .b(b0), .cin(cin), .sum(fa0_sum), .cout(fa0_cout));
    wire fa1_sum;
    wire fa1_cout;
    FullAdder fa1 (.a(a1), .b(b1), .cin(fa0_cout), .sum(fa1_sum), .cout(fa1_cout));
    assign sum0 = fa0_sum;
    assign sum1 = fa1_sum;
    assign cout = fa1_cout;
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

