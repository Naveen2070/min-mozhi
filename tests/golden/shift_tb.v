module shift_widens_past_the_operand_width_tb;
  reg [4-1:0] din;
  wire [8-1:0] literal_shift;
  wire [8-1:0] param_shift;
  wire [8-1:0] var_shift;

  Shift  #(.AMOUNT(2)) _dut_inst (
    .din(din),
    .literal_shift(literal_shift),
    .param_shift(param_shift),
    .var_shift(var_shift)
  );


  initial begin
    $dumpfile("shift_widens_past_the_operand_width_tb.vcd");
    $dumpvars(0, shift_widens_past_the_operand_width_tb);
    din = 0;
    din = 3;
    if (!((literal_shift == 8))) begin
      $display("FAIL: expect %0s failed", "(literal_shift == 8)");
      $finish;
    end
    if (!((param_shift == 12))) begin
      $display("FAIL: expect %0s failed", "(param_shift == 12)");
      $finish;
    end
    if (!((var_shift == 12))) begin
      $display("FAIL: expect %0s failed", "(var_shift == 12)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

