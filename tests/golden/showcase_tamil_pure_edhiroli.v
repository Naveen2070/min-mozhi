module ethiroli #(
    parameter CLKS_PER_BIT = 4
) (
    input wire katikai,
    input wire miittal,
    input wire vaangki,
    output wire anuppi,
    output wire nerukkam,
    output wire [(8)-1:0] perrrrathu
);
    wire [7:0] __mimz_sub_1;
    assign __mimz_sub_1 = (nakarvu >> 1);
    wire [7:0] __mimz_sub_2;
    assign __mimz_sub_2 = ((vaangki) << 7);
    localparam [1:0] VAANGKINILAI_SEYALARRRRA = 0;
    localparam [1:0] VAANGKINILAI_THOTAKKAM = 1;
    localparam [1:0] VAANGKINILAI_THARAVU = 2;
    localparam [1:0] VAANGKINILAI_NIRRUTHTHAM = 3;
    reg [1:0] nilai_pathivu;
    reg [(16)-1:0] veeka_ennnnikkai;
    reg [(3)-1:0] thunnmi_kurriyiitu;
    reg [(8)-1:0] nakarvu;
    reg [(8)-1:0] vaangki_ennnnezhuththu;
    reg [(8)-1:0] ethiroli_ennnnezhuththu;
    reg ethiroli_niluvai;
    reg anuppi_thotakkam;
    wire anuppi_nikazhvu_tx;
    wire anuppi_nikazhvu_busy;
    UartTx #(.CLKS_PER_BIT(CLKS_PER_BIT)) anuppi_nikazhvu (.clk(clk), .rst(rst), .start(anuppi_thotakkam), .data(ethiroli_ennnnezhuththu), .tx(anuppi_nikazhvu_tx), .busy(anuppi_nikazhvu_busy));
    assign anuppi = anuppi_nikazhvu_tx;
    assign nerukkam = (((nilai_pathivu == VAANGKINILAI_SEYALARRRRA)) ? (anuppi_nikazhvu_busy) : (1));
    assign perrrrathu = vaangki_ennnnezhuththu;
    always @(posedge katikai) begin
        if (miittal) begin
            anuppi_thotakkam <= 0;
            veeka_ennnnikkai <= 0;
            thunnmi_kurriyiitu <= 0;
            nilai_pathivu <= VAANGKINILAI_SEYALARRRRA;
            nakarvu <= 0;
            vaangki_ennnnezhuththu <= 0;
            ethiroli_ennnnezhuththu <= 0;
            ethiroli_niluvai <= 0;
        end else begin
            anuppi_thotakkam <= 0;
            if ((nilai_pathivu == VAANGKINILAI_SEYALARRRRA)) begin
                veeka_ennnnikkai <= 0;
                thunnmi_kurriyiitu <= 0;
                if ((vaangki == 0)) begin
                    nilai_pathivu <= VAANGKINILAI_THOTAKKAM;
                end else begin
                    nilai_pathivu <= VAANGKINILAI_SEYALARRRRA;
                end
            end else begin
                if ((nilai_pathivu == VAANGKINILAI_THOTAKKAM)) begin
                    thunnmi_kurriyiitu <= 0;
                    veeka_ennnnikkai <= (veeka_ennnnikkai + 1);
                    if ((veeka_ennnnikkai == (CLKS_PER_BIT - 1))) begin
                        veeka_ennnnikkai <= 0;
                        if ((vaangki == 0)) begin
                            nilai_pathivu <= VAANGKINILAI_THARAVU;
                        end else begin
                            nilai_pathivu <= VAANGKINILAI_SEYALARRRRA;
                        end
                    end else begin
                        nilai_pathivu <= VAANGKINILAI_THOTAKKAM;
                    end
                end else begin
                    if ((nilai_pathivu == VAANGKINILAI_THARAVU)) begin
                        veeka_ennnnikkai <= (veeka_ennnnikkai + 1);
                        if ((veeka_ennnnikkai == (CLKS_PER_BIT - 1))) begin
                            veeka_ennnnikkai <= 0;
                            nakarvu <= (__mimz_sub_1 | __mimz_sub_2);
                            thunnmi_kurriyiitu <= (thunnmi_kurriyiitu + 1);
                            if ((thunnmi_kurriyiitu == 7)) begin
                                nilai_pathivu <= VAANGKINILAI_NIRRUTHTHAM;
                            end else begin
                                nilai_pathivu <= VAANGKINILAI_THARAVU;
                            end
                        end else begin
                            nilai_pathivu <= VAANGKINILAI_THARAVU;
                        end
                    end else begin
                        thunnmi_kurriyiitu <= 0;
                        veeka_ennnnikkai <= (veeka_ennnnikkai + 1);
                        if ((veeka_ennnnikkai == (CLKS_PER_BIT - 1))) begin
                            veeka_ennnnikkai <= 0;
                            if ((vaangki == 1)) begin
                                vaangki_ennnnezhuththu <= nakarvu;
                                ethiroli_ennnnezhuththu <= nakarvu;
                                ethiroli_niluvai <= 1;
                            end
                            nilai_pathivu <= VAANGKINILAI_SEYALARRRRA;
                        end else begin
                            nilai_pathivu <= VAANGKINILAI_NIRRUTHTHAM;
                        end
                    end
                end
            end
            if (ethiroli_niluvai) begin
                anuppi_thotakkam <= 1;
                ethiroli_niluvai <= 0;
            end
        end
    end
endmodule

module UartTx #(
    parameter CLKS_PER_BIT = 4
) (
    input wire clk,
    input wire rst,
    input wire start,
    input wire [(8)-1:0] data,
    output wire tx,
    output wire busy
);
    localparam [1:0] STATE_IDLE = 0;
    localparam [1:0] STATE_START = 1;
    localparam [1:0] STATE_DATA = 2;
    localparam [1:0] STATE_STOP = 3;
    reg [1:0] state;
    reg [(16)-1:0] clk_count;
    reg [(3)-1:0] bit_index;
    reg [(8)-1:0] shift;
    assign tx = (((state == STATE_IDLE)) ? (1) : (((state == STATE_START)) ? (0) : (((state == STATE_DATA)) ? (shift[0]) : (1))));
    assign busy = (((state == STATE_IDLE)) ? (0) : (((state == STATE_START)) ? (1) : (((state == STATE_DATA)) ? (1) : (1))));
    always @(posedge clk) begin
        if (rst) begin
            clk_count <= 0;
            bit_index <= 0;
            shift <= 0;
            state <= STATE_IDLE;
        end else begin
            if ((state == STATE_IDLE)) begin
                clk_count <= 0;
                bit_index <= 0;
                if (start) begin
                    shift <= data;
                    state <= STATE_START;
                end
            end else begin
                if ((state == STATE_START)) begin
                    if ((clk_count == (CLKS_PER_BIT - 1))) begin
                        clk_count <= 0;
                        state <= STATE_DATA;
                    end else begin
                        clk_count <= (clk_count + 1);
                    end
                end else begin
                    if ((state == STATE_DATA)) begin
                        if ((clk_count == (CLKS_PER_BIT - 1))) begin
                            clk_count <= 0;
                            shift <= (shift >> 1);
                            if ((bit_index == 7)) begin
                                bit_index <= 0;
                                state <= STATE_STOP;
                            end else begin
                                bit_index <= (bit_index + 1);
                            end
                        end else begin
                            clk_count <= (clk_count + 1);
                        end
                    end else begin
                        if ((clk_count == (CLKS_PER_BIT - 1))) begin
                            clk_count <= 0;
                            state <= STATE_IDLE;
                        end else begin
                            clk_count <= (clk_count + 1);
                        end
                    end
                end
            end
        end
    end
endmodule

