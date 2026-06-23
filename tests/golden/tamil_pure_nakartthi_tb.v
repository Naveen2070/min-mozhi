module _________________________________tb;
  reg [4-1:0] tharavu;
  wire [8-1:0] maarrilinakarvu;
  wire [8-1:0] allavunakarvu;
  wire [8-1:0] maarrinakarvu;

  nakarththi  #(.allavu(2)) _dut_inst (
    .tharavu(tharavu),
    .maarrilinakarvu(maarrilinakarvu),
    .allavunakarvu(allavunakarvu),
    .maarrinakarvu(maarrinakarvu)
  );


  initial begin
    $dumpfile("_________________________________tb.vcd");
    $dumpvars(0, _________________________________tb);
    tharavu = 0;
    tharavu = 3;
    if (!((maarrilinakarvu == 8))) begin
      $display("FAIL: expect %0s failed", "(maarrilinakarvu == 8)");
      $finish;
    end
    if (!((allavunakarvu == 12))) begin
      $display("FAIL: expect %0s failed", "(allavunakarvu == 12)");
      $finish;
    end
    if (!((maarrinakarvu == 12))) begin
      $display("FAIL: expect %0s failed", "(maarrinakarvu == 12)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

