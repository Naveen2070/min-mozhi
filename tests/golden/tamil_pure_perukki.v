module perukki (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] aa,
    output wire [(16)-1:0] vitai
);
    function automatic [(16)-1:0] perukku;
        input [(8)-1:0] a;
        input [(8)-1:0] aa;
        begin
            perukku = (a * aa);
        end
    endfunction
    assign vitai = perukku(a, aa);
endmodule

