module ________3______________tb;
  reg [4-1:0] ilakkam;
  wire [7-1:0] kaatsi;

  ennkaatti _dut_inst (
    .ilakkam(ilakkam),
    .kaatsi(kaatsi)
  );


  initial begin
    $dumpfile("________3______________tb.vcd");
    $dumpvars(0, ________3______________tb);
    ilakkam = 0;
    ilakkam = 3;
    if (!((kaatsi == 'h4F))) begin
      $display("FAIL: expect %0s failed", "(kaatsi == 'h4F)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module _8_______________________________________tb;
  reg [4-1:0] ilakkam;
  wire [7-1:0] kaatsi;

  ennkaatti _dut_inst (
    .ilakkam(ilakkam),
    .kaatsi(kaatsi)
  );


  initial begin
    $dumpfile("_8_______________________________________tb.vcd");
    $dumpvars(0, _8_______________________________________tb);
    ilakkam = 0;
    ilakkam = 8;
    if (!((kaatsi == 'h7F))) begin
      $display("FAIL: expect %0s failed", "(kaatsi == 'h7F)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module _______________________________________tb;
  reg [4-1:0] ilakkam;
  wire [7-1:0] kaatsi;

  ennkaatti _dut_inst (
    .ilakkam(ilakkam),
    .kaatsi(kaatsi)
  );


  initial begin
    $dumpfile("_______________________________________tb.vcd");
    $dumpvars(0, _______________________________________tb);
    ilakkam = 0;
    ilakkam = 'hF;
    if (!((kaatsi == 'h00))) begin
      $display("FAIL: expect %0s failed", "(kaatsi == 'h00)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

