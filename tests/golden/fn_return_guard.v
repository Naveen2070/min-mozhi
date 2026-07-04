module FindFirstSet (
    input wire [(8)-1:0] a,
    output wire signed [(4)-1:0] idx
);
    function automatic signed [(4)-1:0] find_first_set;
        input [(8)-1:0] a;
        begin
            if ((a[0] == 1)) begin
                find_first_set = 0;
            end else begin
            if ((a[1] == 1)) begin
                find_first_set = 1;
            end else begin
            if ((a[2] == 1)) begin
                find_first_set = 2;
            end else begin
            if ((a[3] == 1)) begin
                find_first_set = 3;
            end else begin
            if ((a[4] == 1)) begin
                find_first_set = 4;
            end else begin
            if ((a[5] == 1)) begin
                find_first_set = 5;
            end else begin
            if ((a[6] == 1)) begin
                find_first_set = 6;
            end else begin
            if ((a[7] == 1)) begin
                find_first_set = 7;
            end else begin
            find_first_set = (-1);
            end
            end
            end
            end
            end
            end
            end
            end
        end
    endfunction
    assign idx = find_first_set(a);
endmodule

