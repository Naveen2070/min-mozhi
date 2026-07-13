module ForeachSum (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] b,
    input wire [(8)-1:0] c,
    input wire [(8)-1:0] d,
    input wire [(8)-1:0] e,
    input wire [(8)-1:0] f,
    input wire [(8)-1:0] g,
    input wire [(8)-1:0] h,
    output wire [(11)-1:0] total
);
    function automatic [(11)-1:0] sum8;
        input [(8)-1:0] values_0;
        input [(8)-1:0] values_1;
        input [(8)-1:0] values_2;
        input [(8)-1:0] values_3;
        input [(8)-1:0] values_4;
        input [(8)-1:0] values_5;
        input [(8)-1:0] values_6;
        input [(8)-1:0] values_7;
        input [(11)-1:0] acc;
        reg [7:0] v;
        reg [10:0] acc;
        begin
            v = values_0;
            acc = (acc + (v));
            v = values_1;
            acc = (acc + (v));
            v = values_2;
            acc = (acc + (v));
            v = values_3;
            acc = (acc + (v));
            v = values_4;
            acc = (acc + (v));
            v = values_5;
            acc = (acc + (v));
            v = values_6;
            acc = (acc + (v));
            v = values_7;
            acc = (acc + (v));
            sum8 = acc;
        end
    endfunction
    assign total = sum8(a, b, c, d, e, f, g, h, 0);
endmodule

