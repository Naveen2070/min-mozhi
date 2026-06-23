module varisai #(
    parameter akalam = 8,
    parameter suttu = 2
) (
    input wire katikai,
    input wire miill,
    input wire nuzhai,
    input wire niikku,
    input wire [(akalam)-1:0] tharavu,
    output wire nirraivu,
    output wire kaali,
    output wire [(akalam)-1:0] vellitharavu
);
    reg [(akalam)-1:0] kallam [0:((1 << suttu))-1];
    reg [(suttu)-1:0] mun;
    reg [(suttu)-1:0] pin;
    reg [((suttu + 1))-1:0] thokai;
    integer __mimz_kallam_i;
    initial for (__mimz_kallam_i = 0; __mimz_kallam_i < ((1 << suttu)); __mimz_kallam_i = __mimz_kallam_i + 1) kallam[__mimz_kallam_i] = 0;
    assign vellitharavu = kallam[mun];
    assign nirraivu = (thokai == (1 << suttu));
    assign kaali = (thokai == 0);
    always @(posedge katikai) begin
        if (miill) begin
            pin <= 0;
            mun <= 0;
            thokai <= 0;
        end else begin
            if ((nuzhai && (thokai != (1 << suttu)))) begin
                kallam[pin] <= tharavu;
                pin <= (pin + 1);
                if ((niikku && (thokai != 0))) begin
                    mun <= (mun + 1);
                end else begin
                    thokai <= (thokai + 1);
                end
            end else begin
                if ((niikku && (thokai != 0))) begin
                    mun <= (mun + 1);
                    thokai <= (thokai - 1);
                end
            end
        end
    end
endmodule

