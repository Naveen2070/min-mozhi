`timescale 1ns/1ps
// Self-checking TB: SyncPulse — proves the `sync.pulse` toggle-based CDC
// pulse synchronizer's crossing latency and single-cycle width against real
// iverilog/vvp (Tasks 1-6). `clk_src` (period 4, edges at 2,6,10,...) and
// `clk_dst` (period 10, edges at 5,15,25,...) are genuinely asynchronous —
// no posedge of either clock ever coincides with a transition of the other,
// and every testbench-driven input change happens at a time that lands on
// neither clock's edge, so it never races a clock edge either.
//
// Per the emitted Verilog (`sync.pulse` lowers to: a `src_reg` sampling the
// `in` port on `clk_src` — required so the signal fed to the primitive is
// owned by its own src_clock domain, per checker rule E0704 — a hidden
// toggle reg XORing itself with `src_reg` on every `clk_src` rise, and a
// 3-stage `clk_dst` shift register whose top two stages are XORed together
// to re-derive a single-cycle pulse): driving `src_pulse` high for exactly
// one `clk_src` cycle flips the toggle reg once; that toggle level then
// takes 2 `clk_dst` edges to propagate through stage0/stage1, at which
// point stage1 XOR stage2 goes high for exactly one `clk_dst` cycle before
// falling back to 0. This mirrors
// `sync_pulse_produces_a_one_cycle_dst_pulse_after_toggle` in
// crates/mimz-sim/src/sim/harness.rs, driven here through the REAL compiled
// Verilog instead of our own kernel.
module sync_pulse_tb;
    reg clk_src = 0, clk_dst = 0, rst = 1;
    reg src_pulse = 0;
    wire dst_pulse;
    integer errors = 0;

    always #2 clk_src = ~clk_src; // period 4
    always #5 clk_dst = ~clk_dst; // period 10, asynchronous to clk_src

    SyncPulse dut (
        .clk_src(clk_src), .clk_dst(clk_dst), .rst(rst),
        .src_pulse(src_pulse), .dst_pulse(dst_pulse)
    );

    task check(input reg expected, input [8*64-1:0] label);
        begin
            if (dst_pulse !== expected) begin
                $display("FAIL: %0s dst_pulse=%0d expected=%0d at t=%0t", label, dst_pulse, expected, $time);
                errors = errors + 1;
            end else begin
                $display("PASS: %0s dst_pulse=%0d", label, dst_pulse);
            end
        end
    endtask

    initial begin
        // Hold reset through several edges of BOTH domains (clk_src: 2,6,10,
        // 14,18,22; clk_dst: 5,15) before releasing at a safe (non-edge) time.
        #23 rst = 0;
        check(0, "after reset, no pulse yet");

        // Drive src_pulse high for exactly one clk_src cycle: asserted from
        // t=23 (covering the clk_src edge at t=26, which samples src_pulse
        // into src_reg) through t=28, well before the NEXT clk_src edge at
        // t=30.
        src_pulse = 1;
        #5 src_pulse = 0; // t=28

        // src_reg and the hidden toggle reg are updated by two SEPARATE
        // always @(posedge clk_src) blocks, so nonblocking-assignment
        // semantics mean the toggle block still reads src_reg's PRE-edge
        // value at t=26 (0) and stays put; src_reg becomes 1 only after
        // t=26. The toggle actually flips one clk_src cycle later, at
        // t=30, once the toggle block reads src_reg=1. Two subsequent
        // clk_dst edges (t=35, t=45) are needed to shift that flip through
        // stage0/stage1: dst_pulse must still read 0 just after t=35 (only
        // stage0 has caught it), go high for exactly one clk_dst cycle
        // after t=45, and fall back to 0 by t=55.
        #10 check(0, "still 0 after t=35 (only stage0 has the toggle)");
        #10 check(1, "single-cycle pulse two dst edges after the toggle (t=45)");
        #10 check(0, "pulse has fallen back to 0 by t=55");

        $display("%0s", errors == 0 ? "PASS" : "FAIL");
        $finish;
    end
endmodule
