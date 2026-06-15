// Self-checking TB: theervi — the pure-Tamil 4-way mux (examples/tamil-pure),
// the same circuit as Mux4 with Tamil names. Every select routes the right
// input. Ports: sel=theervu, a=a, b=aa, c=i, d=ii, y=villaivu.
`timescale 1ns/1ps
module thervi_tb;
  reg [1:0] sel;
  reg [7:0] a, b, c, d;
  wire [7:0] y;
  theervi dut (.theervu(sel), .a(a), .aa(b), .i(c), .ii(d), .villaivu(y));

  task check(input [1:0] s, input [7:0] want);
    begin
      sel = s; #1;
      if (y !== want) begin
        $display("FAIL: sel=%0d y=%h, expected %h", s, y, want);
        $finish;
      end
    end
  endtask

  initial begin
    a = 8'h11; b = 8'h22; c = 8'h33; d = 8'h44;
    check(2'd0, 8'h11);
    check(2'd1, 8'h22);
    check(2'd2, 8'h33);
    check(2'd3, 8'h44);
    $display("PASS");
    $finish;
  end
endmodule
