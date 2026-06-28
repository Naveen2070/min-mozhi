module ConstLocal (
    input wire [(8)-1:0] a,
    output wire [(8)-1:0] result
);
    function automatic [(8)-1:0] add_offset;
        input [(8)-1:0] a;
        reg [2:0] n;
        begin
            n = 5;
            add_offset = (a + n);
        end
    endfunction
    assign result = add_offset(a);
endmodule

