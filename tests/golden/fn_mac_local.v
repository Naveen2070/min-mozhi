module WrapMul (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] b,
    output wire [(8)-1:0] result
);
    function automatic [(8)-1:0] wrap_mul;
        input [(8)-1:0] a;
        input [(8)-1:0] b;
        reg [7:0] p;
        begin
            p = (a * b);
            wrap_mul = p;
        end
    endfunction
    assign result = wrap_mul(a, b);
endmodule

