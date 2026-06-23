module seg7_decodes_3_tb;
  reg [4-1:0] digit;
  wire [7-1:0] seg;

  Seg7 _dut_inst (
    .digit(digit),
    .seg(seg)
  );


  initial begin
    $dumpfile("seg7_decodes_3_tb.vcd");
    $dumpvars(0, seg7_decodes_3_tb);
    digit = 0;
    digit = 3;
    if (!((seg == 'h4F))) begin
      $display("FAIL: expect %0s failed", "(seg == 'h4F)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module seg7_lights_all_segments_for_8_tb;
  reg [4-1:0] digit;
  wire [7-1:0] seg;

  Seg7 _dut_inst (
    .digit(digit),
    .seg(seg)
  );


  initial begin
    $dumpfile("seg7_lights_all_segments_for_8_tb.vcd");
    $dumpvars(0, seg7_lights_all_segments_for_8_tb);
    digit = 0;
    digit = 8;
    if (!((seg == 'h7F))) begin
      $display("FAIL: expect %0s failed", "(seg == 'h7F)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

module seg7_blanks_a_non_decimal_input_tb;
  reg [4-1:0] digit;
  wire [7-1:0] seg;

  Seg7 _dut_inst (
    .digit(digit),
    .seg(seg)
  );


  initial begin
    $dumpfile("seg7_blanks_a_non_decimal_input_tb.vcd");
    $dumpvars(0, seg7_blanks_a_non_decimal_input_tb);
    digit = 0;
    digit = 'hF;
    if (!((seg == 'h00))) begin
      $display("FAIL: expect %0s failed", "(seg == 'h00)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

