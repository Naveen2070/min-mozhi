module Mac (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] b,
    output wire [(16)-1:0] result
);
    function automatic [(16)-1:0] mac;
        input [(8)-1:0] a;
        input [(8)-1:0] b;
        begin
            mac = (a * b);
        end
    endfunction
    assign result = mac(a, b);
endmodule

