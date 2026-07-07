`timescale 1ns/1ps
module sc_melody_player_tb;
    reg clk = 0;
    reg rst = 1;
    reg start;
    wire audio, playing;
    MelodyPlayer dut (.clk(clk), .rst(rst), .start(start), .audio(audio), .playing(playing));
    always #5 clk = ~clk;
    task tick; begin @(posedge clk); #1; end endtask
    initial begin
        start = 0;
        tick();                             // reset cycle 0
        rst = 0;

        // cyc 1: idle (not playing, audio silent)
        if (playing !== 0 || audio !== 0) begin
            $display("FAIL: cyc 1 idle playing=%b audio=%b", playing, audio);
            $finish;
        end

        // cyc 2: start the melody
        start = 1; tick(); start = 0;
        if (playing !== 1) begin
            $display("FAIL: cyc 2 start: playing=%b", playing);
            $finish;
        end

        // cyc 3..102: should keep playing for many cycles
        repeat (100) tick();
        if (playing !== 1) begin
            $display("FAIL: stopped early");
            $finish;
        end

        $display("PASS");
        $finish;
    end
endmodule
