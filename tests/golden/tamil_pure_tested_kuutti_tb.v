module ______________________tb;
  reg [4-1:0] a;
  reg [4-1:0] aa;
  wire [5-1:0] kuuttu;

  kuutti  #(.akalam(4)) _dut_inst (
    .a(a),
    .aa(aa),
    .kuuttu(kuuttu)
  );


  initial begin
    $dumpfile("______________________tb.vcd");
    $dumpvars(0, ______________________tb);
    a = 0;
    aa = 0;
    a = 5;
    aa = 10;
    if (!((kuuttu == 15))) begin
      $display("FAIL: expect %0s failed", "(kuuttu == 15)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

