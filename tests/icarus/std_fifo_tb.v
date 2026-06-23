// Self-checking TB: Fifo — a synchronous ring-buffer queue (defaults WIDTH=8,
// AW=2, DEPTH=4). Verifies: empty after reset; FIFO ordering across push/pop;
// full after DEPTH pushes; and that an overflow push is ignored (head datum
// and full flag unchanged).
`timescale 1ns/1ps
module std_fifo_tb;
  reg clk = 0, rst = 1, push = 0, pop = 0;
  reg [7:0] din = 0;
  wire full, empty;
  wire [7:0] dout;
  Fifo dut (.clk(clk), .rst(rst), .push(push), .pop(pop), .din(din),
            .full(full), .empty(empty), .dout(dout));
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

    // Enqueue three bytes; the head exposes the first.
    do_push(8'h11);
    do_push(8'h22);
    do_push(8'h33);
    if (empty !== 1'b0 || full !== 1'b0) begin
      $display("FAIL: after 3 pushes empty=%b full=%b", empty, full); $finish;
    end
    if (dout !== 8'h11) begin $display("FAIL: head=%h expected 11", dout); $finish; end

    // Dequeue in FIFO order: 11, 22, 33.
    do_pop;
    if (dout !== 8'h22) begin $display("FAIL: after pop head=%h expected 22", dout); $finish; end
    do_pop;
    if (dout !== 8'h33) begin $display("FAIL: after pop head=%h expected 33", dout); $finish; end
    do_pop;
    if (empty !== 1'b1) begin $display("FAIL: expected empty after draining"); $finish; end

    // Fill to full (DEPTH=4); the head datum is the first of this batch.
    do_push(8'hA0);
    do_push(8'hA1);
    do_push(8'hA2);
    do_push(8'hA3);
    if (full !== 1'b1) begin $display("FAIL: expected full after 4 pushes"); $finish; end

    // An overflow push is ignored — full stays set, head datum unchanged.
    do_push(8'hFF);
    if (full !== 1'b1) begin $display("FAIL: overflow cleared full"); $finish; end
    if (dout !== 8'hA0) begin $display("FAIL: overflow corrupted head=%h expected A0", dout); $finish; end

    $display("PASS");
    $finish;
  end
endmodule
