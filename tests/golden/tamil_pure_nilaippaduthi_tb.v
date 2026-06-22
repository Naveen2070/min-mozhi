module ___________________________tb;
  reg katikai;
  reg miill;
  reg muulam;
  wire thellivu;

  nilaippatuththi  #(.akalam(3), .urruthi(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .muulam(muulam),
    .thellivu(thellivu)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("___________________________tb.vcd");
    $dumpvars(0, ___________________________tb);
    miill = 0;
    muulam = 0;
    muulam = 1;
    repeat (8) @(posedge katikai);
    if (!((thellivu == 1))) begin
      $display("FAIL: expect %0s failed", "(thellivu == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module ___________________________________tb;
  reg katikai;
  reg miill;
  reg muulam;
  wire thellivu;

  nilaippatuththi  #(.akalam(3), .urruthi(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .muulam(muulam),
    .thellivu(thellivu)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("___________________________________tb.vcd");
    $dumpvars(0, ___________________________________tb);
    miill = 0;
    muulam = 0;
    muulam = 1;
    repeat (2) @(posedge katikai);
    muulam = 0;
    repeat (8) @(posedge katikai);
    if (!((thellivu == 0))) begin
      $display("FAIL: expect %0s failed", "(thellivu == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

