`timescale 1ns/1ps
// Self-checking TB: oththisaitheeti — the pure-Tamil twin of
// `sync_loop_search_tb.v` (english/sync_loop_search.mimz ->
// tamil-pure/sync_loop_search.mimz). Same `sync loop` FSM start->done timing
// and result-latching coverage, through the romanized identifiers:
// clk/rst/key/find_first_start/found/busy/find_first_done ->
// katikai/miill/visai/muthaltheetu_start/itam/iyangkum/muthaltheetu_done.
// `nem anni` is left zero-initialized (same MVP scope as the English
// original), so this drives key=0 (matches every index — the loop body has
// no "already found" guard, so the LAST index checked (7) is what's latched)
// and key=0xFF (matches none).
module oththisaitheeti_tb;
    reg clk = 0;
    reg rst;
    reg [7:0] key;
    reg start;
    wire signed [3:0] found;
    wire busy;
    wire done;
    integer i;
    integer errors = 0;

    always #5 clk = ~clk;

    oththisaitheeti dut (
        .katikai(clk), .miill(rst), .visai(key),
        .muthaltheetu_start(start), .itam(found), .iyangkum(busy), .muthaltheetu_done(done)
    );

    task run_search(input [7:0] k, input signed [3:0] expected);
        begin
            @(posedge clk); #1; rst = 1;
            @(posedge clk); #1; rst = 0;
            key = k;
            start = 1;
            @(posedge clk); #1;
            start = 0;
            for (i = 0; i < 8; i = i + 1) begin
                @(posedge clk); #1;
            end
            if (!done || found !== expected) begin
                $display("FAIL: key=%0d expected=%0d got found=%0d done=%0d", k, expected, found, done);
                errors = errors + 1;
            end else begin
                $display("PASS: key=%0d found=%0d", k, found);
            end
        end
    endtask

    initial begin
        rst = 1; start = 0; key = 0;
        run_search(8'h00, 7);
        run_search(8'hFF, -1); // no match against an all-zero mem
        $display(errors == 0 ? "PASS" : "FAIL");
        $finish;
    end
endmodule
