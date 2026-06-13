// Self-checking TB: Window — the chained comparison `lo <= value <= hi`
// desugars to `(lo <= value) && (value <= hi)`. in_range must be 1 exactly
// when value is within [lo, hi] inclusive, including the boundaries and the
// degenerate lo > hi case.
`timescale 1ns / 1ps
module window_tb;
  reg [7:0] lo, value, hi;
  wire in_range;
  Window dut (
      .lo(lo),
      .value(value),
      .hi(hi),
      .in_range(in_range)
  );

  task check(input [7:0] xlo, input [7:0] xval, input [7:0] xhi, input xexp);
    begin
      lo = xlo; value = xval; hi = xhi; #1;
      if (in_range !== xexp) begin
        $display("FAIL: lo=%0d value=%0d hi=%0d -> in_range=%b (expected %b)",
                 xlo, xval, xhi, in_range, xexp);
        $finish;
      end
    end
  endtask

  initial begin
    check(8'd10,  8'd50,  8'd200, 1'b1);  // inside the window
    check(8'd10,  8'd5,   8'd200, 1'b0);  // below lo
    check(8'd10,  8'd250, 8'd200, 1'b0);  // above hi
    check(8'd10,  8'd10,  8'd200, 1'b1);  // on the lower bound
    check(8'd10,  8'd200, 8'd200, 1'b1);  // on the upper bound
    check(8'd100, 8'd50,  8'd20,  1'b0);  // degenerate range (lo > hi)
    $display("PASS");
    $finish;
  end
endmodule
