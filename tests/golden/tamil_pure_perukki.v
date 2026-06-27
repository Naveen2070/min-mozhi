module perukki (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] aa,
    output wire [(16)-1:0] vitai
);
    function automatic [(16)-1:0] mac;
        input [(8)-1:0] a;
        input [(8)-1:0] aa;
        begin
            mac = (a * aa);
        end
    endfunction
    assign vitai = mac(a, aa);
endmodule

