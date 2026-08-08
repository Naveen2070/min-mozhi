`timescale 1ns/1ps
module sc_pid_controller_tb;
    reg clk = 0;
    reg rst = 1;
    reg signed [7:0] setpoint, measured;
    wire signed [7:0] control;
    wire saturated;
    PidController dut (.clk(clk), .rst(rst), .setpoint(setpoint), .measured(measured), .control(control), .saturated(saturated));
    always #5 clk = ~clk;
    task tick; begin @(posedge clk); #1; end endtask
    initial begin
        setpoint = 0; measured = 0; tick();
        rst = 0;
        // cyc 1: zero error → control=0, sat=0 (integral=0, p=0, d=0)
        if (control !== 0 || saturated !== 0) begin
            $display("FAIL: cyc 1 zero err: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 2: set=10, meas=0 → err=10; prev_error catches up to err on
        // this same edge, so d=err-prev_error=0 by the time we check; p=20,
        // int=0(old)+10=10, total=(p+int)+d=30.
        setpoint = 10; measured = 0; tick();
        if (control !== 30 || saturated !== 0) begin
            $display("FAIL: cyc 2 pos: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 3: same inputs → err=10, d=0 (prev_error already caught up
        // last cycle); integral is CUMULATIVE (`integral <- integral +
        // extend(error, 16)` every edge, no reset while err≠0), so
        // int=10(old)+10=20, total=(p=20+int=20)+d=0=40.
        tick();
        if (control !== 40 || saturated !== 0) begin
            $display("FAIL: cyc 3 steady: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 4: set=0, meas=10 → err=-10, p=-20, d=0 (prev_error catches
        // up again), int=20(old)-10=10, total=(-20+10)+0=-10.
        setpoint = 0; measured = 10; tick();
        if (control !== -10 || saturated !== 0) begin
            $display("FAIL: cyc 4 neg: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 5: set=100, meas=0 → err=100, p=200, d=0, int=10(old)+100=110,
        // total=(200+110)+0=310, clamped to 127, sat=1.
        setpoint = 100; measured = 0; tick();
        if (control !== 127 || saturated !== 1) begin
            $display("FAIL: cyc 5 sat: control=%0d sat=%b", control, saturated);
            $finish;
        end
        $display("PASS");
        $finish;
    end
endmodule
