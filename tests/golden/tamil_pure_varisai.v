module varisai #(
    parameter akalam = 8,
    parameter aazham = 4
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
    function integer clog2;
        input integer value;
        integer i;
        begin
            if (value <= 1) clog2 = 1;
            else begin
                clog2 = 0;
                for (i = value - 1; i > 0; i = i >> 1) clog2 = clog2 + 1;
            end
        end
    endfunction
    reg [(akalam)-1:0] kallam [0:(aazham)-1];
    reg [(clog2(aazham))-1:0] mun;
    reg [(clog2(aazham))-1:0] pin;
    reg [((clog2(aazham) + 1))-1:0] thokai;
    integer __mimz_kallam_i;
    initial for (__mimz_kallam_i = 0; __mimz_kallam_i < (aazham); __mimz_kallam_i = __mimz_kallam_i + 1) kallam[__mimz_kallam_i] = 0;
    assign vellitharavu = kallam[mun];
    assign nirraivu = (thokai == aazham);
    assign kaali = (thokai == 0);
    always @(posedge katikai) begin
        if (miill) begin
            pin <= 0;
            mun <= 0;
            thokai <= 0;
        end else begin
            if ((nuzhai && (thokai != aazham))) begin
                kallam[pin] <= tharavu;
                if ((pin == (aazham - 1))) begin
                    pin <= 0;
                end else begin
                    pin <= (pin + 1);
                end
                if ((niikku && (thokai != 0))) begin
                    if ((mun == (aazham - 1))) begin
                        mun <= 0;
                    end else begin
                        mun <= (mun + 1);
                    end
                end else begin
                    thokai <= (thokai + 1);
                end
            end else begin
                if ((niikku && (thokai != 0))) begin
                    if ((mun == (aazham - 1))) begin
                        mun <= 0;
                    end else begin
                        mun <= (mun + 1);
                    end
                    thokai <= (thokai - 1);
                end
            end
        end
    end
endmodule

