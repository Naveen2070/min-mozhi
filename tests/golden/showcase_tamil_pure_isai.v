module isai #(
    parameter TICK = 50000
) (
    input wire katikai,
    input wire miittal,
    input wire thotakkam,
    output wire oli,
    output wire iyakkam
);
    reg [(4)-1:0] mukavari;
    reg [(20)-1:0] suram_ennnnikkai;
    reg [(32)-1:0] kaala_ennnnikkai;
    reg maarrrru;
    reg seyalil;
    reg [(20)-1:0] ilakku;
    wire [(20)-1:0] suruthi;
    wire [(8)-1:0] kaalam;
    assign suruthi = (((mukavari == 0)) ? (75758) : (((mukavari == 1)) ? (75758) : (((mukavari == 2)) ? (71633) : (((mukavari == 3)) ? (63776) : (((mukavari == 4)) ? (63776) : (((mukavari == 5)) ? (71633) : (((mukavari == 6)) ? (75758) : (((mukavari == 7)) ? (85034) : (((mukavari == 8)) ? (95420) : (((mukavari == 9)) ? (95420) : (((mukavari == 10)) ? (85034) : (((mukavari == 11)) ? (75758) : (((mukavari == 12)) ? (85034) : (((mukavari == 13)) ? (85034) : (((mukavari == 14)) ? (95420) : (0))))))))))))))));
    assign kaalam = (((mukavari == 0)) ? (10) : (((mukavari == 1)) ? (10) : (((mukavari == 2)) ? (10) : (((mukavari == 3)) ? (15) : (((mukavari == 4)) ? (10) : (((mukavari == 5)) ? (10) : (((mukavari == 6)) ? (15) : (((mukavari == 7)) ? (10) : (((mukavari == 8)) ? (15) : (((mukavari == 9)) ? (10) : (((mukavari == 10)) ? (10) : (((mukavari == 11)) ? (10) : (((mukavari == 12)) ? (5) : (((mukavari == 13)) ? (5) : (((mukavari == 14)) ? (20) : (1))))))))))))))));
    assign oli = maarrrru;
    assign iyakkam = seyalil;
    always @(posedge katikai) begin
        if (miittal) begin
            seyalil <= 0;
            mukavari <= 0;
            kaala_ennnnikkai <= 0;
            suram_ennnnikkai <= 0;
            maarrrru <= 0;
            ilakku <= 0;
        end else begin
            if ((thotakkam && (!seyalil))) begin
                seyalil <= 1;
                mukavari <= 0;
                kaala_ennnnikkai <= 0;
                suram_ennnnikkai <= 0;
                maarrrru <= 0;
                ilakku <= 0;
            end
            if (seyalil) begin
                if ((kaala_ennnnikkai == 0)) begin
                    ilakku <= suruthi;
                    kaala_ennnnikkai <= ((kaalam) * TICK)[(32)-1:0];
                    mukavari <= (mukavari + 1);
                    if ((mukavari == 15)) begin
                        seyalil <= 0;
                    end
                    suram_ennnnikkai <= 0;
                    maarrrru <= 0;
                end else begin
                    kaala_ennnnikkai <= (kaala_ennnnikkai - 1);
                    if ((ilakku != 0)) begin
                        suram_ennnnikkai <= (suram_ennnnikkai + 1);
                        if ((suram_ennnnikkai == ilakku)) begin
                            suram_ennnnikkai <= 0;
                            maarrrru <= (~maarrrru);
                        end
                    end
                end
            end
        end
    end
endmodule

