module CanTherivu (
    input wire katikai,
    input wire miill,
    input wire [(2)-1:0] inam,
    input wire unnmai,
    output wire vati,
    output wire [(2)-1:0] satta_inam,
    output wire [(4)-1:0] sari_ennnnikkai
);
    reg [(4)-1:0] tharavu_ennnnikkai;
    reg [(4)-1:0] tholai_ennnnikkai;
    reg [(4)-1:0] pizhai_ennnnikkai;
    reg vati_pathiveetu;
    reg [(2)-1:0] satta_pathiveetu;
    reg [(4)-1:0] sari_pathiveetu;
    wire eerrpu;
    assign eerrpu = (((inam == 0)) ? (1) : (((inam == 1)) ? (1) : (0)));
    assign vati = vati_pathiveetu;
    assign satta_inam = satta_pathiveetu;
    assign sari_ennnnikkai = sari_pathiveetu;
    always @(posedge katikai) begin
        if (miill) begin
            vati_pathiveetu <= 0;
            satta_pathiveetu <= 0;
            sari_pathiveetu <= 0;
            tharavu_ennnnikkai <= 0;
            tholai_ennnnikkai <= 0;
            pizhai_ennnnikkai <= 0;
        end else begin
            vati_pathiveetu <= 0;
            satta_pathiveetu <= 0;
            if (unnmai) begin
                satta_pathiveetu <= inam;
                if (eerrpu) begin
                    vati_pathiveetu <= 1;
                    sari_pathiveetu <= (sari_pathiveetu + 1);
                end
                if ((inam == 0)) begin
                    tharavu_ennnnikkai <= (tharavu_ennnnikkai + 1);
                end
                if ((inam == 1)) begin
                    tholai_ennnnikkai <= (tholai_ennnnikkai + 1);
                end
                if ((inam == 2)) begin
                    pizhai_ennnnikkai <= (pizhai_ennnnikkai + 1);
                end
            end
        end
    end
endmodule

