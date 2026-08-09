module nakarththi #(
    parameter allavu = 2
) (
    input wire [(4)-1:0] tharavu,
    output wire [(8)-1:0] maarrilinakarvu,
    output wire [(8)-1:0] allavunakarvu,
    output wire [(8)-1:0] maarrinakarvu
);
    wire [3:0] __mimz_sub_1;
    assign __mimz_sub_1 = (1 << 3);
    wire [5:0] __mimz_sub_2;
    assign __mimz_sub_2 = (tharavu << 2);
    assign maarrilinakarvu = (__mimz_sub_1);
    assign allavunakarvu = ((3 << allavu));
    assign maarrinakarvu = (__mimz_sub_2);
endmodule

