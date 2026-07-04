module FindIndex (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] b,
    input wire [(8)-1:0] c,
    input wire [(8)-1:0] d,
    input wire [(8)-1:0] target,
    input wire [(3)-1:0] pick_idx,
    output wire signed [(4)-1:0] idx,
    output wire [(8)-1:0] picked
);
    function automatic signed [(4)-1:0] find_index;
        input [(8)-1:0] vals_0;
        input [(8)-1:0] vals_1;
        input [(8)-1:0] vals_2;
        input [(8)-1:0] vals_3;
        input [(8)-1:0] target;
        begin
            if ((vals_0 == target)) begin
                find_index = 0;
            end else begin
            if ((vals_1 == target)) begin
                find_index = 1;
            end else begin
            if ((vals_2 == target)) begin
                find_index = 2;
            end else begin
            if ((vals_3 == target)) begin
                find_index = 3;
            end else begin
            find_index = (-1);
            end
            end
            end
            end
        end
    endfunction
    function automatic [(8)-1:0] pick;
        input [(8)-1:0] vals_0;
        input [(8)-1:0] vals_1;
        input [(8)-1:0] vals_2;
        input [(8)-1:0] vals_3;
        input [(3)-1:0] idx;
        begin
            pick = ((idx)==0) ? vals_0 : (((idx)==1) ? vals_1 : (((idx)==2) ? vals_2 : (vals_3)));
        end
    endfunction
    assign idx = find_index(a, b, c, d, target);
    assign picked = pick(a, b, c, d, pick_idx);
endmodule

