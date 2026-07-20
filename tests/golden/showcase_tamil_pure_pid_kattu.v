module PidKattu (
    input wire katikai,
    input wire miittal,
    input wire signed [(8)-1:0] ilakku_mathippu,
    input wire signed [(8)-1:0] allavitappattathu,
    output wire signed [(8)-1:0] kattuppaatu,
    output wire nirraivu
);
    wire signed [9:0] __mimz_sub_1;
    assign __mimz_sub_1 = ((ilakku_mathippu) - (allavitappattathu));
    wire signed [10:0] __mimz_sub_2;
    assign __mimz_sub_2 = ((pizhai) + (pizhai));
    wire signed [10:0] __mimz_sub_3;
    assign __mimz_sub_3 = ((pizhai) - (munthaiya_pizhai));
    wire signed [16:0] __mimz_sub_4;
    assign __mimz_sub_4 = ((vikitha_urruppu) + thokaiyiitu);
    wire signed [16:0] __mimz_sub_5;
    assign __mimz_sub_5 = (__mimz_sub_4[(16)-1:0] + (vikitha_veerrupaatu));
    wire signed [16:0] __mimz_sub_6;
    assign __mimz_sub_6 = (thokaiyiitu + (pizhai));
    reg signed [(16)-1:0] thokaiyiitu;
    reg signed [(8)-1:0] munthaiya_pizhai;
    wire signed [(9)-1:0] pizhai;
    wire signed [(10)-1:0] vikitha_urruppu;
    wire signed [(10)-1:0] vikitha_veerrupaatu;
    wire signed [(16)-1:0] moththam;
    assign pizhai = __mimz_sub_1[(9)-1:0];
    assign vikitha_urruppu = __mimz_sub_2[(10)-1:0];
    assign vikitha_veerrupaatu = __mimz_sub_3[(10)-1:0];
    assign moththam = __mimz_sub_5[(16)-1:0];
    assign kattuppaatu = (((-128) < ((moththam < 127) ? (moththam) : (127))) ? (((moththam < 127) ? (moththam) : (127))) : ((-128)))[(8)-1:0];
    assign nirraivu = ((moththam < (-128)) || (moththam > 127));
    always @(posedge katikai) begin
        if (miittal) begin
            thokaiyiitu <= 0;
            munthaiya_pizhai <= 0;
        end else begin
            thokaiyiitu <= thokaiyiitu;
            thokaiyiitu <= __mimz_sub_6[(16)-1:0];
            munthaiya_pizhai <= pizhai[(8)-1:0];
        end
    end
endmodule

