module anuppi #(
    parameter kannakku = 4
) (
    input wire katikai,
    input wire miill,
    input wire thotangku,
    input wire [(8)-1:0] tharavu,
    output wire vari,
    output wire veelai
);
    localparam [1:0] NILAI_OOYVU = 0;
    localparam [1:0] NILAI_THOTAKKAM = 1;
    localparam [1:0] NILAI_THAKAVAL = 2;
    localparam [1:0] NILAI_MUTIVU = 3;
    reg [1:0] natappu;
    reg [(16)-1:0] ennnni;
    reg [(3)-1:0] suttu;
    reg [(8)-1:0] nakarvu;
    assign vari = (((natappu == NILAI_OOYVU)) ? (1) : (((natappu == NILAI_THOTAKKAM)) ? (0) : (((natappu == NILAI_THAKAVAL)) ? (nakarvu[0]) : (1))));
    assign veelai = (((natappu == NILAI_OOYVU)) ? (0) : (((natappu == NILAI_THOTAKKAM)) ? (1) : (((natappu == NILAI_THAKAVAL)) ? (1) : (1))));
    always @(posedge katikai) begin
        if (miill) begin
            ennnni <= 0;
            suttu <= 0;
            nakarvu <= 0;
            natappu <= NILAI_OOYVU;
        end else begin
            if ((natappu == NILAI_OOYVU)) begin
                ennnni <= 0;
                suttu <= 0;
                if (thotangku) begin
                    nakarvu <= tharavu;
                    natappu <= NILAI_THOTAKKAM;
                end
            end else begin
                if ((natappu == NILAI_THOTAKKAM)) begin
                    if ((ennnni == (kannakku - 1))) begin
                        ennnni <= 0;
                        natappu <= NILAI_THAKAVAL;
                    end else begin
                        ennnni <= (ennnni + 1);
                    end
                end else begin
                    if ((natappu == NILAI_THAKAVAL)) begin
                        if ((ennnni == (kannakku - 1))) begin
                            ennnni <= 0;
                            nakarvu <= (nakarvu >> 1);
                            if ((suttu == 7)) begin
                                suttu <= 0;
                                natappu <= NILAI_MUTIVU;
                            end else begin
                                suttu <= (suttu + 1);
                            end
                        end else begin
                            ennnni <= (ennnni + 1);
                        end
                    end else begin
                        if ((ennnni == (kannakku - 1))) begin
                            ennnni <= 0;
                            natappu <= NILAI_OOYVU;
                        end else begin
                            ennnni <= (ennnni + 1);
                        end
                    end
                end
            end
        end
    end
endmodule

