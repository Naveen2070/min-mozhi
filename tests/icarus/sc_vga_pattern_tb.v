`timescale 1ns/1ps
module sc_vga_pattern_tb;
    reg clk = 0;
    reg rst = 1;
    wire vga_hsync, vga_vsync;
    wire [1:0] vga_red, vga_green, vga_blue;
    wire [3:0] frame_count;
    reg frame_cnt_start;
    wire frame_cnt_done;
    wire [3:0] frame_cnt_result;
    wire frame_cnt_running;
    VgaPattern dut (
        .clk(clk), .rst(rst),
        .vga_hsync(vga_hsync), .vga_vsync(vga_vsync),
        .vga_red(vga_red), .vga_green(vga_green), .vga_blue(vga_blue),
        .frame_count(frame_count),
        .frame_cnt_start(frame_cnt_start), .frame_cnt_done(frame_cnt_done),
        .frame_cnt_result(frame_cnt_result), .frame_cnt_running(frame_cnt_running)
    );
    always #5 clk = ~clk;
    task tick; begin @(posedge clk); #1; end endtask
    initial begin
        frame_cnt_start = 0; tick(); rst = 0;

        // cyc 1: after reset h_cnt=0, v_cnt=0 → hsync=1 (idle high)
        if (vga_hsync !== 1) begin
            $display("FAIL: cyc 1 hsync=%b expected 1", vga_hsync);
            $finish;
        end
        if (vga_vsync !== 0) begin
            $display("FAIL: cyc 1 vsync=%b expected 0", vga_vsync);
            $finish;
        end

        // Wait until pixel 656 where hsync goes low (H_VISIBLE+H_FRONT = 640+16)
        repeat (655) tick();
        if (vga_hsync !== 0) begin
            $display("FAIL: pixel 656 hsync=%b expected 0 (vga_red=%b grn=%b blu=%b)",
                vga_hsync, vga_red, vga_green, vga_blue);
            $finish;
        end

        // Wait through the sync pulse (96 pixels) — at pixel 752 hsync goes high
        repeat (96) tick();
        if (vga_hsync !== 1) begin
            $display("FAIL: pixel 752 hsync=%b expected 1", vga_hsync);
            $finish;
        end

        // Back porch → hsync should stay high
        repeat (48) tick();
        if (vga_hsync !== 1) begin
            $display("FAIL: pixel 800 hsync=%b expected 1", vga_hsync);
            $finish;
        end

        $display("PASS");
        $finish;
    end
endmodule
