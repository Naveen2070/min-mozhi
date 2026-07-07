`timescale 1ns/1ps
module sc_can_frame_filter_tb;
    reg clk = 0;
    reg rst = 1;
    reg [1:0] kind;
    reg valid;
    wire filter_out;
    wire [1:0] frame_type;
    wire [3:0] match_count;
    CanFrameFilter dut (
        .clk(clk), .rst(rst), .kind(kind), .valid(valid),
        .filter_out(filter_out), .frame_type(frame_type), .match_count(match_count)
    );
    always #5 clk = ~clk;
    task tick; begin @(posedge clk); #1; end endtask
    task checkc(input [31:0] cyc, input want_f, input [1:0] want_ft, input [3:0] want_mc);
        if (filter_out !== want_f || frame_type !== want_ft || match_count !== want_mc) begin
            $display("FAIL: cyc %0d filter=%b type=%b cnt=%0d (exp %b %b %0d)",
                cyc, filter_out, frame_type, match_count, want_f, want_ft, want_mc);
            $finish;
        end
    endtask
    initial begin
        rst = 1; kind = 0; valid = 0; tick();
        rst = 0;
        kind = 0; valid = 1; tick(); checkc(1, 1, 0, 1);
        kind = 0; valid = 0; tick(); checkc(2, 0, 0, 1);
        kind = 1; valid = 1; tick(); checkc(3, 1, 1, 2);
        kind = 2; valid = 1; tick(); checkc(4, 0, 2, 2);
        kind = 3; valid = 1; tick(); checkc(5, 0, 3, 2);
        $display("PASS");
        $finish;
    end
endmodule
