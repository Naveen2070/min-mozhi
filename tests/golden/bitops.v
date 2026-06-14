module BitOps (
    input wire [(4)-1:0] a,
    input wire [(4)-1:0] b,
    input wire signed [(4)-1:0] s,
    output wire [(4)-1:0] lo,
    output wire [(4)-1:0] hi,
    output wire signed [(5)-1:0] mag,
    output wire nd,
    output wire nr,
    output wire xn
);
    assign lo = ((a < b) ? (a) : (b));
    assign hi = ((a < b) ? (b) : (a));
    assign mag = ((s < 0) ? (-s) : (s));
    assign nd = (~&(a));
    assign nr = (~|(a));
    assign xn = (~^(a));
endmodule

