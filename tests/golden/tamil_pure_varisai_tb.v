module ____________________tb;
  reg katikai;
  reg miill;
  reg nuzhai;
  reg niikku;
  reg [8-1:0] tharavu;
  wire nirraivu;
  wire kaali;
  wire [8-1:0] vellitharavu;

  varisai  #(.akalam(8), .suttu(2), .aazham(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .nuzhai(nuzhai),
    .niikku(niikku),
    .tharavu(tharavu),
    .nirraivu(nirraivu),
    .kaali(kaali),
    .vellitharavu(vellitharavu)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("____________________tb.vcd");
    $dumpvars(0, ____________________tb);
    miill = 0;
    nuzhai = 0;
    niikku = 0;
    tharavu = 0;
    if (!((kaali == 1))) begin
      $display("FAIL: expect %0s failed", "(kaali == 1)");
      $finish;
    end
    if (!((nirraivu == 0))) begin
      $display("FAIL: expect %0s failed", "(nirraivu == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module _____________________________________tb;
  reg katikai;
  reg miill;
  reg nuzhai;
  reg niikku;
  reg [8-1:0] tharavu;
  wire nirraivu;
  wire kaali;
  wire [8-1:0] vellitharavu;

  varisai  #(.akalam(8), .suttu(2), .aazham(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .nuzhai(nuzhai),
    .niikku(niikku),
    .tharavu(tharavu),
    .nirraivu(nirraivu),
    .kaali(kaali),
    .vellitharavu(vellitharavu)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("_____________________________________tb.vcd");
    $dumpvars(0, _____________________________________tb);
    miill = 0;
    nuzhai = 0;
    niikku = 0;
    tharavu = 0;
    nuzhai = 1;
    tharavu = 'hA5;
    repeat (1) @(posedge katikai);
    nuzhai = 0;
    if (!((vellitharavu == 'hA5))) begin
      $display("FAIL: expect %0s failed", "(vellitharavu == 'hA5)");
      $finish;
    end
    if (!((kaali == 0))) begin
      $display("FAIL: expect %0s failed", "(kaali == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module ______________________________tb;
  reg katikai;
  reg miill;
  reg nuzhai;
  reg niikku;
  reg [8-1:0] tharavu;
  wire nirraivu;
  wire kaali;
  wire [8-1:0] vellitharavu;

  varisai  #(.akalam(8), .suttu(2), .aazham(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .nuzhai(nuzhai),
    .niikku(niikku),
    .tharavu(tharavu),
    .nirraivu(nirraivu),
    .kaali(kaali),
    .vellitharavu(vellitharavu)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("______________________________tb.vcd");
    $dumpvars(0, ______________________________tb);
    miill = 0;
    nuzhai = 0;
    niikku = 0;
    tharavu = 0;
    nuzhai = 1;
    tharavu = 1;
    repeat (4) @(posedge katikai);
    if (!((nirraivu == 1))) begin
      $display("FAIL: expect %0s failed", "(nirraivu == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

