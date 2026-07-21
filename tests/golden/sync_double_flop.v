module SyncDoubleFlop (
    input wire clk_src,
    input wire clk_dst,
    input wire rst,
    input wire fast_bit,
    output wire slow_bit
);
    reg fast_reg;
    reg synced;
    reg __sync_synced_stage0;
    assign slow_bit = synced;
    always @(posedge clk_src) begin
        if (rst) begin
            fast_reg <= 0;
        end else begin
            fast_reg <= fast_bit;
        end
    end
    always @(posedge clk_dst) begin
        if (rst) begin
            __sync_synced_stage0 <= 0;
            synced <= 0;
        end else begin
            __sync_synced_stage0 <= fast_reg;
            synced <= __sync_synced_stage0;
        end
    end
endmodule

