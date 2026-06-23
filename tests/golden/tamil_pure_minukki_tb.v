module ___________________________________________tb;
  reg katikai;
  reg miill;
  reg [4-1:0] katamai;
  wire alai;

  minukki  #(.akalam(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .katamai(katamai),
    .alai(alai)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("___________________________________________tb.vcd");
    $dumpvars(0, ___________________________________________tb);
    miill = 0;
    katamai = 0;
    katamai = 0;
    repeat (6) @(posedge katikai);
    if (!((alai == 0))) begin
      $display("FAIL: expect %0s failed", "(alai == 0)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module ____________________________________________tb;
  reg katikai;
  reg miill;
  reg [4-1:0] katamai;
  wire alai;

  minukki  #(.akalam(4)) _dut_inst (
    .katikai(katikai),
    .miill(miill),
    .katamai(katamai),
    .alai(alai)
  );

  initial katikai = 0;
  always #5 katikai = ~katikai;

  initial begin
    $dumpfile("____________________________________________tb.vcd");
    $dumpvars(0, ____________________________________________tb);
    miill = 0;
    katamai = 0;
    katamai = 8;
    if (!((alai == 1))) begin
      $display("FAIL: expect %0s failed", "(alai == 1)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

