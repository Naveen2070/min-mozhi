module _______________tb;
  reg katikai;
  reg miill;
  reg thotangku;
  reg [8-1:0] tharavu;
  wire vari;
  wire veelai;

  anuppi  #(.kannakku(2)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .thotangku(thotangku),
    .tharavu(tharavu),
    .vari(vari),
    .veelai(veelai)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("_______________tb.vcd");
    $dumpvars(0, _______________tb);
    miill = 0;
    thotangku = 0;
    tharavu = 0;
    if (!((vari == 1))) begin
      $display("FAIL: expect %0s failed", "(vari == 1)");
      $finish;
    end
    if (!((veelai == 0))) begin
      $display("FAIL: expect %0s failed", "(veelai == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module ______________________________________________tb;
  reg katikai;
  reg miill;
  reg thotangku;
  reg [8-1:0] tharavu;
  wire vari;
  wire veelai;

  anuppi  #(.kannakku(2)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .thotangku(thotangku),
    .tharavu(tharavu),
    .vari(vari),
    .veelai(veelai)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("______________________________________________tb.vcd");
    $dumpvars(0, ______________________________________________tb);
    miill = 0;
    thotangku = 0;
    tharavu = 0;
    thotangku = 1;
    tharavu = 'h55;
    repeat (1) @(posedge katikai);
    if (!((veelai == 1))) begin
      $display("FAIL: expect %0s failed", "(veelai == 1)");
      $finish;
    end
    if (!((vari == 0))) begin
      $display("FAIL: expect %0s failed", "(vari == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module __________________________________________tb;
  reg katikai;
  reg miill;
  reg thotangku;
  reg [8-1:0] tharavu;
  wire vari;
  wire veelai;

  anuppi  #(.kannakku(2)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .thotangku(thotangku),
    .tharavu(tharavu),
    .vari(vari),
    .veelai(veelai)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("__________________________________________tb.vcd");
    $dumpvars(0, __________________________________________tb);
    miill = 0;
    thotangku = 0;
    tharavu = 0;
    thotangku = 1;
    tharavu = 'h55;
    repeat (1) @(posedge katikai);
    thotangku = 0;
    repeat (24) @(posedge katikai);
    if (!((veelai == 0))) begin
      $display("FAIL: expect %0s failed", "(veelai == 0)");
      $finish;
    end
    if (!((vari == 1))) begin
      $display("FAIL: expect %0s failed", "(vari == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

