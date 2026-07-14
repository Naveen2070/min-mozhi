module moththam (
    input wire [(8)-1:0] a,
    input wire [(8)-1:0] aa,
    input wire [(8)-1:0] i,
    input wire [(8)-1:0] ii,
    input wire [(8)-1:0] u,
    input wire [(8)-1:0] uu,
    input wire [(8)-1:0] e,
    input wire [(8)-1:0] ee,
    output wire [(11)-1:0] vitai
);
    function automatic [(11)-1:0] kuuttu;
        input [(8)-1:0] anni_0;
        input [(8)-1:0] anni_1;
        input [(8)-1:0] anni_2;
        input [(8)-1:0] anni_3;
        input [(8)-1:0] anni_4;
        input [(8)-1:0] anni_5;
        input [(8)-1:0] anni_6;
        input [(8)-1:0] anni_7;
        input [(11)-1:0] thokai;
        reg [7:0] mathi;
        begin
            mathi = anni_0;
            thokai = (thokai + (mathi));
            mathi = anni_1;
            thokai = (thokai + (mathi));
            mathi = anni_2;
            thokai = (thokai + (mathi));
            mathi = anni_3;
            thokai = (thokai + (mathi));
            mathi = anni_4;
            thokai = (thokai + (mathi));
            mathi = anni_5;
            thokai = (thokai + (mathi));
            mathi = anni_6;
            thokai = (thokai + (mathi));
            mathi = anni_7;
            thokai = (thokai + (mathi));
            kuuttu = thokai;
        end
    endfunction
    assign vitai = kuuttu(a, aa, i, ii, u, uu, e, ee, 0);
endmodule

