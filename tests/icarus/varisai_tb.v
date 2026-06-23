// Self-checking TB: varisai — the pure-Tamil FIFO (examples/tamil-pure), the
// same circuit as Fifo with Tamil names. Ports are the romanized Tamil names:
// clk=katikai, rst=miill, push=nuzhai, pop=niikku, din=tharavu,
// full=nirraivu, empty=kaali, dout=vellitharavu.
`timescale 1ns/1ps
module varisai_tb;
  reg clk = 0, rst = 1, push = 0, pop = 0;
  reg [7:0] din = 0;
  wire full, empty;
  wire [7:0] dout;
  varisai dut (.katikai(clk), .miill(rst), .nuzhai(push), .niikku(pop),
               .tharavu(din), .nirraivu(full), .kaali(empty), .vellitharavu(dout));
  always #5 clk = ~clk;

  task do_push(input [7:0] v);
    begin push = 1; din = v; @(posedge clk); #1; push = 0; end
  endtask
  task do_pop;
    begin pop = 1; @(posedge clk); #1; pop = 0; end
  endtask

  initial begin
    @(posedge clk); #1; rst = 0;
    if (empty !== 1'b1 || full !== 1'b0) begin
      $display("FAIL: after reset empty=%b full=%b", empty, full); $finish;
    end

    do_push(8'h11);
    do_push(8'h22);
    do_push(8'h33);
    if (dout !== 8'h11) begin $display("FAIL: head=%h expected 11", dout); $finish; end

    do_pop;
    if (dout !== 8'h22) begin $display("FAIL: after pop head=%h expected 22", dout); $finish; end
    do_pop;
    if (dout !== 8'h33) begin $display("FAIL: after pop head=%h expected 33", dout); $finish; end
    do_pop;
    if (empty !== 1'b1) begin $display("FAIL: expected empty after draining"); $finish; end

    do_push(8'hA0);
    do_push(8'hA1);
    do_push(8'hA2);
    do_push(8'hA3);
    if (full !== 1'b1) begin $display("FAIL: expected full after 4 pushes"); $finish; end

    do_push(8'hFF);
    if (full !== 1'b1) begin $display("FAIL: overflow cleared full"); $finish; end
    if (dout !== 8'hA0) begin $display("FAIL: overflow corrupted head=%h expected A0", dout); $finish; end

    $display("PASS");
    $finish;
  end
endmodule
