module Replicate (
    input wire [(4)-1:0] a,
    output wire [(8)-1:0] doubled,
    output wire [(12)-1:0] tripled
);
    assign doubled = {2{a}};
    assign tripled = {3{a}};
endmodule

