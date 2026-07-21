`timescale 1ns/1ps
// Self-checking TB: SyncDoubleFlop — proves the `sync.double_flop` two-stage
// CDC synchronizer's crossing latency against real iverilog/vvp (Tasks 1-6).
// `clk_src` (period 4, edges at 2,6,10,...) and `clk_dst` (period 10, edges
// at 5,15,25,...) are genuinely asynchronous — no posedge of either clock
// ever coincides with a transition of the other, and every testbench-driven
// input change happens at an odd time NOT ending in 5, so it never races a
// clock edge either.
//
// Per the emitted Verilog (`sync.double_flop` lowers to a hidden stage reg
// plus the destination reg the source expression is assigned into): a value
// written to `fast_reg` on a `clk_src` edge needs TWO subsequent `clk_dst`
// edges to reach `slow_bit` — one to land in the hidden stage, one more to
// land in `synced` (the classic 2-flop synchronizer). This mirrors
// `sync_double_flop_settles_after_two_dst_clock_cycles` in
// crates/mimz-sim/src/sim/harness.rs, driven here through the REAL compiled
// Verilog instead of our own kernel.
module sync_double_flop_tb;
    reg clk_src = 0, clk_dst = 0, rst = 1;
    reg fast_bit = 0;
    wire slow_bit;
    integer errors = 0;

    always #2 clk_src = ~clk_src; // period 4
    always #5 clk_dst = ~clk_dst; // period 10, asynchronous to clk_src

    SyncDoubleFlop dut (
        .clk_src(clk_src), .clk_dst(clk_dst), .rst(rst),
        .fast_bit(fast_bit), .slow_bit(slow_bit)
    );

    task check(input reg expected, input [8*40-1:0] label);
        begin
            if (slow_bit !== expected) begin
                $display("FAIL: %0s slow_bit=%0d expected=%0d at t=%0t", label, slow_bit, expected, $time);
                errors = errors + 1;
            end else begin
                $display("PASS: %0s slow_bit=%0d", label, slow_bit);
            end
        end
    endtask

    initial begin
        // Hold reset through several edges of BOTH domains (clk_src: 2,6,10,
        // 14,18,22; clk_dst: 5,15) before releasing at a safe (non-edge) time.
        #23 rst = 0;
        check(0, "after reset, no drive yet");

        // Drive fast_bit high; two clk_dst edges after the clk_src edge that
        // latches it into fast_reg, slow_bit must read back 1.
        fast_bit = 1;
        #24 check(1, "fast_bit=1 crossed to slow_bit");

        // Drop it and confirm the same latency crossing it back to 0.
        fast_bit = 0;
        #20 check(0, "fast_bit=0 crossed to slow_bit");

        $display("%0s", errors == 0 ? "PASS" : "FAIL");
        $finish;
    end
endmodule
