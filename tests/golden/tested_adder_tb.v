module adder_works_tb;
  reg [4-1:0] a;
  reg [4-1:0] b;
  wire [5-1:0] sum;

  Adder  #(.WIDTH(4)) _dut_inst (
    .a(a),
    .b(b),
    .sum(sum)
  );


  initial begin
    $dumpfile("adder_works_tb.vcd");
    $dumpvars(0, adder_works_tb);
    a = 0;
    b = 0;
    a = 5;
    b = 10;
    if (!((sum == 15))) begin
      $display("FAIL: expect %0s failed", "(sum == 15)");
      $finish;
    end
    $display("PASS");
    $finish;
  end
endmodule

