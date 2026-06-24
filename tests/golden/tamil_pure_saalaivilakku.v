module saalaivillakku (
    input wire katikai,
    input wire miill,
    output wire sivappu,
    output wire manjsall,
    output wire passai
);
    localparam [1:0] NILAI_NIRRUTHTHU = 0;
    localparam [1:0] NILAI_SEL = 1;
    localparam [1:0] NILAI_ESSARI = 2;
    reg [1:0] natappu;
    reg [(8)-1:0] neeram;
    assign sivappu = (natappu == NILAI_NIRRUTHTHU);
    assign manjsall = (natappu == NILAI_ESSARI);
    assign passai = (natappu == NILAI_SEL);
    always @(posedge katikai) begin
        if (miill) begin
            natappu <= NILAI_NIRRUTHTHU;
            neeram <= 0;
        end else begin
            if ((neeram == 0)) begin
                natappu <= (((natappu == NILAI_NIRRUTHTHU)) ? (NILAI_SEL) : (((natappu == NILAI_SEL)) ? (NILAI_ESSARI) : (NILAI_NIRRUTHTHU)));
                neeram <= (((natappu == NILAI_NIRRUTHTHU)) ? (50) : (((natappu == NILAI_SEL)) ? (40) : (10)));
            end else begin
                neeram <= (neeram - 1);
            end
        end
    end
endmodule

