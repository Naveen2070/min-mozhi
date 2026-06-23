module nakarththi #(
    parameter allavu = 2
) (
    input wire [(4)-1:0] tharavu,
    output wire [(8)-1:0] maarrilinakarvu,
    output wire [(8)-1:0] allavunakarvu,
    output wire [(8)-1:0] maarrinakarvu
);
    assign maarrilinakarvu = ((1 << 3));
    assign allavunakarvu = ((3 << allavu));
    assign maarrinakarvu = ((tharavu << 2));
endmodule

