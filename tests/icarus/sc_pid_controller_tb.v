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
        // cyc 2: set=10, meas=0 → err=10, p=20, d=10 (prev_err=0), total=30
        setpoint = 10; measured = 0; tick();
        if (control !== 30 || saturated !== 0) begin
            $display("FAIL: cyc 2 pos: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 3: same inputs → p=20, d=0 (err-prev=10-10), int=10, total=30
        tick();
        if (control !== 30 || saturated !== 0) begin
            $display("FAIL: cyc 3 steady: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 4: set=0, meas=10 → err=-10, p=-20, d=-20 (err-prev=-10-10),
        // int=20, total=(-20+20)+(-20)=-20, control=-20
        setpoint = 0; measured = 10; tick();
        if (control !== -20 || saturated !== 0) begin
            $display("FAIL: cyc 4 neg: control=%0d sat=%b", control, saturated);
            $finish;
        end
        // cyc 5: set=100, meas=0 → err=100, p=200, d=110 (100-(-10)),
        // int=10, total=(200+10)+110=320, clamp(320)=127, sat=1
        setpoint = 100; measured = 0; tick();
        if (control !== 127 || saturated !== 1) begin
            $display("FAIL: cyc 5 sat: control=%0d sat=%b", control, saturated);
            $finish;
        end
        $display("PASS");
        $finish;
    end
endmodule
