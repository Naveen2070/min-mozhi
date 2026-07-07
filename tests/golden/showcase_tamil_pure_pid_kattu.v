module PidKattu (
    input wire katikai,
    input wire miittal,
    input wire signed [(8)-1:0] ilakku_mathippu,
    input wire signed [(8)-1:0] allavitappattathu,
    output wire signed [(8)-1:0] kattuppaatu,
    output wire nirraivu
);
    reg signed [(16)-1:0] thokaiyiitu;
    reg signed [(8)-1:0] munthaiya_pizhai;
    wire signed [(9)-1:0] pizhai;
    wire signed [(10)-1:0] vikitha_urruppu;
    wire signed [(10)-1:0] vikitha_veerrupaatu;
    wire signed [(16)-1:0] moththam;
    assign pizhai = ((ilakku_mathippu) - (allavitappattathu))[(9)-1:0];
    assign vikitha_urruppu = ((pizhai) + (pizhai))[(10)-1:0];
    assign vikitha_veerrupaatu = ((pizhai) - (munthaiya_pizhai))[(10)-1:0];
    assign moththam = (((vikitha_urruppu) + thokaiyiitu)[(16)-1:0] + (vikitha_veerrupaatu))[(16)-1:0];
    assign kattuppaatu = (((-128) < ((moththam < 127) ? (moththam) : (127))) ? (((moththam < 127) ? (moththam) : (127))) : ((-128)))[(8)-1:0];
    assign nirraivu = ((moththam < (-128)) || (moththam > 127));
    always @(posedge katikai) begin
        if (miittal) begin
            thokaiyiitu <= 0;
            munthaiya_pizhai <= 0;
        end else begin
            thokaiyiitu <= thokaiyiitu;
            thokaiyiitu <= (thokaiyiitu + (pizhai))[(16)-1:0];
            munthaiya_pizhai <= pizhai[(8)-1:0];
        end
    end
endmodule

