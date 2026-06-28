module Scaled (
    input wire [(8)-1:0] a,
    output wire [(8)-1:0] result
);
    function automatic [(8)-1:0] scaled;
        input [(8)-1:0] a;
        begin
            scaled = (a >> 3);
        end
    endfunction
    assign result = scaled(a);
endmodule

