module vilakku_emulation_tb;
  reg manni;
  reg miill;
  wire olli;

  villakku _dut_inst (
    .manni(manni),
    .miill(miill),
    .olli(olli)
  );

  initial manni = 0;
  always #5 manni = ~manni;

  initial begin
    $dumpfile("vilakku_emulation_tb.vcd");
    $dumpvars(0, vilakku_emulation_tb);
    miill = 0;
    miill = 1;
    repeat (1) @(posedge manni);
    miill = 0;
    repeat (10) @(posedge manni);
    $display("PASS");
    $finish;
  end
endmodule

