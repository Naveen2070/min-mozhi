module SyncPulse (
    input wire clk_src,
    input wire clk_dst,
    input wire rst,
    input wire src_pulse,
    output wire dst_pulse
);
    reg src_reg;
    reg __sync_dst_pulse_w_toggle;
    reg __sync_dst_pulse_w_stage0;
    reg __sync_dst_pulse_w_stage1;
    reg __sync_dst_pulse_w_stage2;
    wire dst_pulse_w;
    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` register-init line(s) below are simulation/FPGA-only - an ASIC flow has no defined power-on default and will not honor them. The synchronous reset below still applies regardless.
    initial #0 src_reg = 0;
    initial #0 __sync_dst_pulse_w_toggle = 0;
    initial #0 __sync_dst_pulse_w_stage0 = 0;
    initial #0 __sync_dst_pulse_w_stage1 = 0;
    initial #0 __sync_dst_pulse_w_stage2 = 0;
    assign dst_pulse_w = (__sync_dst_pulse_w_stage1 ^ __sync_dst_pulse_w_stage2);
    assign dst_pulse = dst_pulse_w;
    always @(posedge clk_src) begin
        if (rst) begin
            src_reg <= 0;
        end else begin
            src_reg <= src_pulse;
        end
    end
    always @(posedge clk_src) begin
        if (rst) begin
            __sync_dst_pulse_w_toggle <= 0;
        end else begin
            __sync_dst_pulse_w_toggle <= (__sync_dst_pulse_w_toggle ^ src_reg);
        end
    end
    always @(posedge clk_dst) begin
        if (rst) begin
            __sync_dst_pulse_w_stage0 <= 0;
            __sync_dst_pulse_w_stage1 <= 0;
            __sync_dst_pulse_w_stage2 <= 0;
        end else begin
            __sync_dst_pulse_w_stage0 <= __sync_dst_pulse_w_toggle;
            __sync_dst_pulse_w_stage1 <= __sync_dst_pulse_w_stage0;
            __sync_dst_pulse_w_stage2 <= __sync_dst_pulse_w_stage1;
        end
    end
endmodule

