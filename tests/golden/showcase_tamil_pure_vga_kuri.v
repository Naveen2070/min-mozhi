module VgaKuri (
    input wire katikai,
    input wire miittal,
    output wire vga_hsync,
    output wire vga_vsync,
    output wire [1:0] vga_red,
    output wire [1:0] vga_green,
    output wire [1:0] vga_blue,
    output wire [(4)-1:0] frame_count,
    input wire frame_cnt_start,
    output wire frame_cnt_done,
    output wire [(4)-1:0] frame_cnt_result,
    output wire frame_cnt_running
);
    wire [5:0] __mimz_sub_1;
    assign __mimz_sub_1 = (16 - 1);
    reg [(10)-1:0] h_cnt;
    reg [(10)-1:0] v_cnt;
    wire hsync;
    wire vsync;
    wire h_active;
    wire v_active;
    wire blank;
    wire [(2)-1:0] bar_sel;
    wire invert;
    reg [(4)-1:0] frame_cnt_cnt;
    reg frame_cnt_running_r;
    reg [(4)-1:0] frame_cnt_acc;
    reg frame_cnt_done_r;
    assign hsync = ((((h_cnt >= (640 + 16)) && (h_cnt < ((640 + 16) + 96)))) ? (0) : (1));
    assign vsync = ((((v_cnt >= (480 + 10)) && (v_cnt < ((480 + 10) + 2)))) ? (1) : (0));
    assign h_active = (h_cnt < 640);
    assign v_active = (v_cnt < 480);
    assign blank = ((!h_active) || (!v_active));
    assign bar_sel = (h_cnt >> 7)[(2)-1:0];
    assign invert = v_cnt[4];
    assign vga_hsync = hsync;
    assign vga_vsync = vsync;
    assign vga_red = ((blank) ? (0) : (((invert) ? ((~bar_sel)) : (bar_sel))));
    assign vga_green = ((blank) ? (0) : (((invert) ? (bar_sel) : ((~bar_sel)))));
    assign vga_blue = ((blank) ? (0) : ({bar_sel[0], bar_sel[1]}));
    assign frame_cnt_done = frame_cnt_done_r;
    assign frame_cnt_result = frame_cnt_acc;
    assign frame_cnt_running = frame_cnt_running_r;
    assign frame_count = frame_cnt_result;
    always @(posedge katikai) begin
        if (miittal) begin
            h_cnt <= 0;
            v_cnt <= 0;
        end else begin
            if ((h_cnt == (800 - 1))) begin
                h_cnt <= 0;
                if ((v_cnt == (525 - 1))) begin
                    v_cnt <= 0;
                end else begin
                    v_cnt <= (v_cnt + 1);
                end
            end else begin
                h_cnt <= (h_cnt + 1);
            end
        end
    end
    always @(posedge katikai) begin
        if (miittal) begin
            frame_cnt_running_r <= 0;
            frame_cnt_done_r <= 0;
            frame_cnt_cnt <= 0;
            frame_cnt_acc <= 0;
        end else begin
            if (frame_cnt_running_r) begin
                if ((frame_cnt_cnt == __mimz_sub_1)) begin
                    frame_cnt_running_r <= 0;
                    frame_cnt_done_r <= 1;
                end else begin
                    frame_cnt_cnt <= (frame_cnt_cnt + 1);
                    frame_cnt_done_r <= 0;
                end
                if (((h_cnt == (800 - 1)) && (v_cnt == (525 - 1)))) begin
                    frame_cnt_acc <= frame_cnt_cnt;
                end
            end else begin
                frame_cnt_done_r <= 0;
                if (frame_cnt_start) begin
                    frame_cnt_running_r <= 1;
                    frame_cnt_cnt <= 0;
                    frame_cnt_acc <= 0;
                end
            end
        end
    end
endmodule

