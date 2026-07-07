module MelodyPlayer (
    input wire clk,
    input wire rst,
    input wire start,
    output wire audio,
    output wire playing
);
    reg [(4)-1:0] addr;
    reg [(20)-1:0] tone_cnt;
    reg [(24)-1:0] dur_cnt;
    reg toggle;
    reg active;
    reg [(20)-1:0] target;
    wire [(20)-1:0] pitch;
    wire [(8)-1:0] dur;
    assign pitch = (((addr == 0)) ? (75758) : (((addr == 1)) ? (75758) : (((addr == 2)) ? (71633) : (((addr == 3)) ? (63776) : (((addr == 4)) ? (63776) : (((addr == 5)) ? (71633) : (((addr == 6)) ? (75758) : (((addr == 7)) ? (85034) : (((addr == 8)) ? (95420) : (((addr == 9)) ? (95420) : (((addr == 10)) ? (85034) : (((addr == 11)) ? (75758) : (((addr == 12)) ? (85034) : (((addr == 13)) ? (85034) : (((addr == 14)) ? (95420) : (0))))))))))))))));
    assign dur = (((addr == 0)) ? (10) : (((addr == 1)) ? (10) : (((addr == 2)) ? (10) : (((addr == 3)) ? (15) : (((addr == 4)) ? (10) : (((addr == 5)) ? (10) : (((addr == 6)) ? (15) : (((addr == 7)) ? (10) : (((addr == 8)) ? (15) : (((addr == 9)) ? (10) : (((addr == 10)) ? (10) : (((addr == 11)) ? (10) : (((addr == 12)) ? (5) : (((addr == 13)) ? (5) : (((addr == 14)) ? (20) : (1))))))))))))))));
    assign audio = toggle;
    assign playing = active;
    always @(posedge clk) begin
        if (rst) begin
            active <= 0;
            addr <= 0;
            dur_cnt <= 0;
            tone_cnt <= 0;
            toggle <= 0;
            target <= 0;
        end else begin
            if ((start && (!active))) begin
                active <= 1;
                addr <= 0;
                dur_cnt <= 0;
                tone_cnt <= 0;
                toggle <= 0;
                target <= 0;
            end
            if (active) begin
                if ((dur_cnt == 0)) begin
                    target <= pitch;
                    dur_cnt <= ((dur) * 50000)[(24)-1:0];
                    addr <= (addr + 1);
                    if ((addr == 15)) begin
                        active <= 0;
                    end
                    tone_cnt <= 0;
                    toggle <= 0;
                end else begin
                    dur_cnt <= (dur_cnt - 1);
                    if ((target != 0)) begin
                        tone_cnt <= (tone_cnt + 1);
                        if ((tone_cnt == target)) begin
                            tone_cnt <= 0;
                            toggle <= (~toggle);
                        end
                    end
                end
            end
        end
    end
endmodule

