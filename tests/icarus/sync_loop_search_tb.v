`timescale 1ns/1ps
// Self-checking TB: SyncLoopSearch — proves the `sync loop` FSM's
// `start` -> `done` timing and result-latching against real iverilog/vvp
// (Task 11). `mem m` in the example is left zero-initialized (writing it
// is out of scope for this construct's MVP), so this can only drive
// key=0 (matches every index) and key=0xFF (matches none) — it does NOT
// exercise a genuine "duplicate match, LOWEST index wins" scenario, since
// this loop body has no early-exit/"already found" guard (see run_search
// below for what key=0 actually proves instead). That case is already
// covered for the combinational `loop`/`return` construct by
// fn_array_search_duplicate_match_lower_index_wins_via_icarus in
// tests/icarus.rs.
module sync_loop_search_tb;
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

    SyncLoopSearch dut (
        .clk(clk), .rst(rst), .key(key),
        .find_first_start(start), .found(found), .busy(busy), .find_first_done(done)
    );

    task run_search(input [7:0] k, input signed [3:0] expected);
        begin
            // `#1` after every `@(posedge clk)` avoids the classic testbench/DUT
            // race at the clock edge (this DUT updates its regs with `<=` in the
            // same always block the edge triggers) — same idiom as
            // tests/icarus/counter_tb.v's `@(posedge clk); #1;`.
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
        // mem m is zero-initialized; write-side is out of this construct's
        // scope for the MVP example, so the differential drives `key`
        // against the all-zero memory. key=0 matches EVERY index (0..7) —
        // the loop body has no "already found" guard, so each matching
        // cycle's `result <- ...` unconditionally overwrites the previous
        // one, and the LAST index checked (7) is what's still latched when
        // `done` fires, not the first (see the file header note re: this
        // gap vs. `loop`/`return`'s first-match priority).
        run_search(8'h00, 7);
        run_search(8'hFF, -1); // no match against an all-zero mem
        $display(errors == 0 ? "PASS" : "FAIL");
        $finish;
    end
endmodule
