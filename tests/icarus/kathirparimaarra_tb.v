// Self-checking TB: kathirparimaarra — the pure-Tamil twin of
// `bundle_passthrough.mimz` (tamil-pure/bundle_passthrough.mimz). Plain
// bundle-field passthrough (req.valid/req.data -> rsp.valid/rsp.data),
// through the romanized identifiers: koorikkai_sellu/koorikkai_tharavu
// (in) -> pathil_sellu/pathil_tharavu (out). Not covered by the generic
// `differential()` Layer-3 helper (see `our_simulator_matches_icarus_bit_
// for_bit`'s own comment) since its synthetic bundle-field signal names
// are built from the SOURCE (Tamil) identifiers, not the romanized ones.
`timescale 1ns/1ps
module kathirparimaarra_tb;
  reg sellu;
  reg [7:0] tharavu;
  wire out_sellu;
  wire [7:0] out_tharavu;
  kathirparimaarra dut (
      .koorikkai_sellu(sellu),
      .koorikkai_tharavu(tharavu),
      .pathil_sellu(out_sellu),
      .pathil_tharavu(out_tharavu)
  );

  task check(input xsellu, input [7:0] xtharavu);
    begin
      sellu = xsellu; tharavu = xtharavu; #1;
      if (out_sellu !== xsellu) begin
        $display("FAIL: sellu=%b -> pathil_sellu=%b, expected %b", xsellu, out_sellu, xsellu);
        $finish;
      end
      if (out_tharavu !== xtharavu) begin
        $display("FAIL: tharavu=%0d -> pathil_tharavu=%0d, expected %0d", xtharavu, out_tharavu, xtharavu);
        $finish;
      end
    end
  endtask

  initial begin
    check(1'b1, 8'd42);
    check(1'b0, 8'd0);
    check(1'b1, 8'd255);
    check(1'b0, 8'd128);
    $display("PASS");
    $finish;
  end
endmodule
